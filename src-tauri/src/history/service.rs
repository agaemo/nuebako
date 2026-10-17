//! 履歴サービス。保存の入口(record)、検索、削除、上限処理、起動時の鍵と DB の状態判定を担う。
//!
//! 全件をメモリに持ち(本文は復号済み)、検索と重複判定はメモリ上で行う。
//! 画像本体だけは必要になるまで DB から復号して読む。
//! 時刻の保存は UTC の Unix ミリ秒。

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use super::clock::Clock;
use super::search::{self, first_line, fold, is_blank};
use super::{ClipProvider, ClipRecorder, IgnoreReason, RecordOutcome};
use crate::crypto::cipher::Cipher;
use crate::crypto::keystore::{KeyStore, load_or_create};
use crate::domain::clip::{AppRef, ClipContent, ClipKind, NewClip};
use crate::domain::dto::{ClipDetail, ClipSummary, StorageStatus};
use crate::error::AppError;
use crate::storage::repo::{Payload, Repo, StoredRow, base64_decode, base64_encode};
use crate::storage::schema;
use rusqlite::Connection;

/// テキスト1件の上限(UTF-8 で 1 MiB)。
const MAX_TEXT_BYTES: usize = 1_048_576;
/// 画像1件の上限(10 MiB)。
const MAX_IMAGE_BYTES: usize = 10_485_760;
const DAY_MS: i64 = 86_400_000;
/// 既定の件数上限と保存日数(`Settings` の既定値と同じ)。
const DEFAULT_MAX_ITEMS: u32 = 1000;
const DEFAULT_RETENTION_DAYS: u32 = 30;
/// 画像の行に表示する先頭行。
const IMAGE_LABEL: &str = "(画像)";

/// 検索結果。
pub struct SearchResult {
    /// 履歴の全件数(検索条件に関わらない)
    pub total: u32,
    pub items: Vec<ClipSummary>,
}

/// メモリ上の1件。本文は復号済み。
struct Entry {
    id: i64,
    copied_at_ms: i64,
    kind: ClipKind,
    text: Option<String>,
    /// 検索用に小文字へそろえた本文
    folded: String,
    /// 重複判定用(メモリ上だけ。DB には保存しない)
    text_hash: u64,
    source_app: Option<AppRef>,
    pinned: bool,
    thumb_b64: Option<String>,
    /// 画像本体のハッシュ(暗号化された payload の中に保存する)
    blob_hash: Option<u64>,
    char_count: u32,
}

impl Entry {
    fn from_row(row: StoredRow) -> Self {
        let p = row.payload;
        let text = p.text;
        let (folded, text_hash, char_count) = match &text {
            Some(t) => (fold(t), hash_of(t.as_bytes()), t.chars().count() as u32),
            None => (String::new(), 0, 0),
        };
        Entry {
            id: row.id,
            copied_at_ms: row.copied_at_ms,
            kind: p.kind,
            text,
            folded,
            text_hash,
            source_app: p.source_app,
            pinned: p.pinned,
            thumb_b64: p.thumb_png,
            blob_hash: p.blob_hash,
            char_count,
        }
    }

    fn payload(&self) -> Payload {
        Payload {
            v: 1,
            kind: self.kind,
            text: self.text.clone(),
            source_app: self.source_app.clone(),
            pinned: self.pinned,
            thumb_png: self.thumb_b64.clone(),
            blob_hash: self.blob_hash,
        }
    }

    fn is_image(&self) -> bool {
        self.kind == ClipKind::Image
    }

    fn summary(&self) -> ClipSummary {
        ClipSummary {
            id: self.id,
            kind: self.kind,
            first_line: match &self.text {
                Some(t) => first_line(t),
                None => IMAGE_LABEL.to_string(),
            },
            copied_at_ms: self.copied_at_ms,
            pinned: self.pinned,
            source_app_name: self.source_app.as_ref().map(|a| a.name.clone()),
            char_count: self.char_count,
            thumb_png: self.thumb_b64.clone(),
        }
    }
}

fn hash_of(bytes: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

/// 利用できる状態。
struct Active {
    repo: Repo,
    entries: Vec<Entry>,
    max_items: u32,
    retention_days: u32,
}

enum State {
    Ready(Box<Active>),
    /// 鍵や DB の問題で使えない。全メソッドがこのエラーを返し、DB には触れない。
    Unavailable(AppError),
}

pub struct HistoryService {
    state: Mutex<State>,
    clock: Arc<dyn Clock>,
    skipped: u32,
}

/// 上限(件数・保存期間)を適用すると削除される id を返す。ピン留めは対象外。
/// 保存期間は「経過時間が保存期間を超えた」もの(ちょうどは残す)。未来の時刻は消さない。
/// 件数はピン留めを除いた件数で数え、新しい順に残す。
fn over_limit_ids(entries: &[Entry], max_items: u32, retention_days: u32, now: i64) -> Vec<i64> {
    let cutoff = now.saturating_sub(i64::from(retention_days).saturating_mul(DAY_MS));
    let mut live: Vec<&Entry> = Vec::new();
    let mut ids = Vec::new();
    for e in entries.iter().filter(|e| !e.pinned) {
        if e.copied_at_ms < cutoff {
            ids.push(e.id);
        } else {
            live.push(e);
        }
    }
    live.sort_by(|a, b| b.copied_at_ms.cmp(&a.copied_at_ms).then(b.id.cmp(&a.id)));
    ids.extend(live.iter().skip(max_items as usize).map(|e| e.id));
    ids
}

impl Active {
    fn load(repo: Repo) -> Result<(Self, u32), AppError> {
        let (rows, skipped) = repo.load_all()?;
        let entries = rows.into_iter().map(Entry::from_row).collect();
        Ok((
            Active {
                repo,
                entries,
                max_items: DEFAULT_MAX_ITEMS,
                retention_days: DEFAULT_RETENTION_DAYS,
            },
            skipped,
        ))
    }

    fn index_of(&self, id: i64) -> Result<usize, AppError> {
        self.entries
            .iter()
            .position(|e| e.id == id)
            .ok_or(AppError::NotFound)
    }

    /// 上限を適用して、削除した件数を返す。
    fn apply_limits(
        &mut self,
        max_items: u32,
        retention_days: u32,
        now: i64,
    ) -> Result<u32, AppError> {
        let ids = over_limit_ids(&self.entries, max_items, retention_days, now);
        if ids.is_empty() {
            return Ok(0);
        }
        let n = self.repo.delete_ids(&ids)?;
        self.entries.retain(|e| !ids.contains(&e.id));
        Ok(n)
    }

    fn record_text(
        &mut self,
        kind: ClipKind,
        source_app: Option<AppRef>,
        text: String,
        now: i64,
    ) -> Result<RecordOutcome, AppError> {
        if is_blank(&text) {
            return Ok(RecordOutcome::Ignored(IgnoreReason::Blank));
        }
        if text.len() > MAX_TEXT_BYTES {
            return Ok(RecordOutcome::Ignored(IgnoreReason::TooLarge));
        }
        let hash = hash_of(text.as_bytes());
        if let Some(e) = self.entries.iter_mut().find(|e| {
            !e.is_image() && e.text_hash == hash && e.text.as_deref() == Some(text.as_str())
        }) {
            self.repo.update_copied_at(e.id, now)?;
            e.copied_at_ms = now;
            return Ok(RecordOutcome::Promoted(e.id));
        }
        let payload = Payload::text(kind, &text, source_app);
        let id = self.repo.insert(now, &payload, None)?;
        self.entries.push(Entry::from_row(StoredRow {
            id,
            copied_at_ms: now,
            payload,
        }));
        Ok(RecordOutcome::Added(id))
    }

    fn record_image(
        &mut self,
        source_app: Option<AppRef>,
        png: Vec<u8>,
        thumb_png: Vec<u8>,
        now: i64,
    ) -> Result<RecordOutcome, AppError> {
        if png.is_empty() {
            return Ok(RecordOutcome::Ignored(IgnoreReason::Blank));
        }
        if png.len() > MAX_IMAGE_BYTES {
            return Ok(RecordOutcome::Ignored(IgnoreReason::TooLarge));
        }
        let hash = hash_of(&png);
        let candidates: Vec<i64> = self
            .entries
            .iter()
            .filter(|e| e.is_image() && e.blob_hash == Some(hash))
            .map(|e| e.id)
            .collect();
        for id in candidates {
            if self.repo.load_blob(id)?.as_deref() == Some(png.as_slice()) {
                self.repo.update_copied_at(id, now)?;
                let i = self.index_of(id)?;
                self.entries[i].copied_at_ms = now;
                return Ok(RecordOutcome::Promoted(id));
            }
        }
        let payload = Payload {
            v: 1,
            kind: ClipKind::Image,
            text: None,
            source_app,
            pinned: false,
            thumb_png: Some(base64_encode(&thumb_png)),
            blob_hash: Some(hash),
        };
        let id = self.repo.insert(now, &payload, Some(&png))?;
        self.entries.push(Entry::from_row(StoredRow {
            id,
            copied_at_ms: now,
            payload,
        }));
        Ok(RecordOutcome::Added(id))
    }
}

fn lock_err<T>(_: std::sync::PoisonError<T>) -> AppError {
    AppError::Internal("履歴の内部状態にアクセスできません".to_string())
}

impl HistoryService {
    /// 起動時の状態判定を行って開く。
    ///
    /// | DB | 鍵 | 動作 |
    /// |---|---|---|
    /// | 無 | 無 | 鍵を生成してキーチェーンに保存し、DB を作る |
    /// | 無 | 有 | 既存の鍵で DB を作る |
    /// | 有 | 有・key_check 成功 | 全件を復号して読み込む(復号できない行は読み飛ばす) |
    /// | 有 | 取得失敗・無・key_check 失敗 | 読み込まず、書き込まず、鍵も作らない |
    ///
    /// 戻り値の 2 つ目は状態、3 つ目は利用者に見せる理由(正常時は `None`)。
    /// 使えない状態でも `HistoryService` は返り、全メソッドが理由のエラーを返す。
    ///
    /// 上限(件数・保存期間)は既定値(1,000 件・30 日)で始まる。設定を読んだ後に
    /// `set_limits` か `apply_limits` を呼ぶこと。
    pub fn open(
        db_path: &Path,
        key_store: &dyn KeyStore,
        clock: Arc<dyn Clock>,
    ) -> (Self, StorageStatus, Option<String>) {
        match Self::try_open(db_path, key_store) {
            Ok((active, skipped)) => {
                if skipped > 0 {
                    // 内容は出さず、件数だけを残す
                    eprintln!("履歴の読み込みで復号できない行を {skipped} 件読み飛ばしました");
                }
                let svc = HistoryService {
                    state: Mutex::new(State::Ready(Box::new(active))),
                    clock,
                    skipped,
                };
                (svc, StorageStatus::Ok, None)
            }
            Err(e) => {
                let status = match e {
                    AppError::KeyUnavailable => StorageStatus::KeyUnavailable,
                    AppError::KeyMismatch => StorageStatus::KeyMismatch,
                    _ => StorageStatus::Error,
                };
                let msg = e.message();
                let svc = HistoryService {
                    state: Mutex::new(State::Unavailable(e)),
                    clock,
                    skipped: 0,
                };
                (svc, status, Some(msg))
            }
        }
    }

    fn try_open(db_path: &Path, key_store: &dyn KeyStore) -> Result<(Active, u32), AppError> {
        let exists = db_path.exists();
        // 取得に失敗したら、DB の有無にかかわらず鍵を作らない(既存の鍵を上書きしないため)
        let key = if exists {
            key_store.load()?.ok_or(AppError::KeyUnavailable)?
        } else {
            let key = load_or_create(key_store)?;
            if let Some(dir) = db_path.parent() {
                std::fs::create_dir_all(dir).map_err(|_| {
                    AppError::Storage("保存先のフォルダを作成できません".to_string())
                })?;
            }
            key
        };
        let conn = Connection::open(db_path).map_err(schema::db_err)?;
        let version = schema::user_version(&conn)?;
        let repo = Repo::new(conn, Cipher::new(key));
        if version == 0 {
            // 新規(または空ファイル)。他のテーブルがあるのに版数が 0 の DB は触らない
            if repo.table_count()? != 0 {
                return Err(AppError::Storage(
                    "未知の形式のデータベースです".to_string(),
                ));
            }
            repo.apply_pragmas()?;
            repo.init_new()?;
        } else if version == schema::SCHEMA_VERSION {
            // 鍵を確かめてから設定を適用する(合わない鍵では DB を書き換えない)
            repo.check_key()?;
            repo.apply_pragmas()?;
        } else {
            return Err(AppError::Storage(
                "新しい版のアプリで作られたデータベースのため、読み込めません".to_string(),
            ));
        }
        Active::load(repo)
    }

    /// 開いている `Repo` から作る(読み込み済みの DB 用)。
    pub(crate) fn from_repo(repo: Repo, clock: Arc<dyn Clock>) -> Result<Self, AppError> {
        let (active, skipped) = Active::load(repo)?;
        Ok(HistoryService {
            state: Mutex::new(State::Ready(Box::new(active))),
            clock,
            skipped,
        })
    }

    /// `record` のたびに適用する上限を設定する。設定を読んだ後や、設定の更新後に呼ぶ。
    /// 既存の履歴は削除しない(削除は `apply_limits`)。
    pub fn set_limits(&self, max_items: u32, retention_days: u32) {
        if let Ok(mut g) = self.state.lock()
            && let State::Ready(a) = &mut *g
        {
            a.max_items = max_items;
            a.retention_days = retention_days;
        }
    }

    /// 起動時に復号できず読み飛ばした行の数。
    pub fn skipped_count(&self) -> u32 {
        self.skipped
    }

    /// 利用できる状態のときだけ `f` を呼ぶ。使えない状態では、その理由のエラーを返し DB に触れない。
    fn with_active<T>(
        &self,
        f: impl FnOnce(&mut Active, i64) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut guard: MutexGuard<State> = self.state.lock().map_err(lock_err)?;
        match &mut *guard {
            State::Ready(a) => f(a, self.clock.now_ms()),
            State::Unavailable(e) => Err(e.clone()),
        }
    }

    /// 履歴を検索する。ピン留めを先頭に、残りは新しい順。空のクエリは全件。
    pub fn search(&self, query: &str) -> Result<SearchResult, AppError> {
        self.with_active(|a, _| {
            let q = fold(query);
            let mut hits: Vec<&Entry> = a
                .entries
                .iter()
                .filter(|e| q.is_empty() || (!e.is_image() && search::matches(&e.folded, &q)))
                .collect();
            hits.sort_by(|x, y| {
                y.pinned
                    .cmp(&x.pinned)
                    .then(y.copied_at_ms.cmp(&x.copied_at_ms))
                    .then(y.id.cmp(&x.id))
            });
            Ok(SearchResult {
                total: a.entries.len() as u32,
                items: hits.into_iter().map(Entry::summary).collect(),
            })
        })
    }

    pub fn detail(&self, id: i64) -> Result<ClipDetail, AppError> {
        self.with_active(|a, _| {
            let e = &a.entries[a.index_of(id)?];
            let image_png = if e.is_image() {
                let png = a.repo.load_blob(id)?.ok_or(AppError::NotFound)?;
                Some(base64_encode(&png))
            } else {
                None
            };
            Ok(ClipDetail {
                id,
                kind: e.kind,
                text: e.text.clone(),
                image_png,
                source_app_name: e.source_app.as_ref().map(|s| s.name.clone()),
                copied_at_ms: e.copied_at_ms,
                char_count: e.char_count,
            })
        })
    }

    pub fn delete(&self, id: i64) -> Result<(), AppError> {
        self.with_active(|a, _| {
            let i = a.index_of(id)?;
            a.repo.delete(id)?;
            a.entries.remove(i);
            Ok(())
        })
    }

    /// 全件を削除し、削除した件数を返す(ピン留めも含む)。
    pub fn clear(&self) -> Result<u32, AppError> {
        self.with_active(|a, _| {
            let n = a.repo.delete_all()?;
            a.entries.clear();
            Ok(n)
        })
    }

    pub fn set_pinned(&self, id: i64, pinned: bool) -> Result<(), AppError> {
        self.with_active(|a, _| {
            let i = a.index_of(id)?;
            let mut payload = a.entries[i].payload();
            payload.pinned = pinned;
            a.repo.update_payload(id, &payload)?;
            a.entries[i].pinned = pinned;
            Ok(())
        })
    }

    /// 上限を適用すると削除される件数(ピン留めは除く)。削除はしない。
    pub fn count_over_limits(&self, max_items: u32, retention_days: u32) -> Result<u32, AppError> {
        self.with_active(|a, now| {
            Ok(over_limit_ids(&a.entries, max_items, retention_days, now).len() as u32)
        })
    }

    /// 上限を適用して、削除した件数を返す。以後の `record` もこの上限を使う。
    pub fn apply_limits(&self, max_items: u32, retention_days: u32) -> Result<u32, AppError> {
        self.with_active(|a, now| {
            a.max_items = max_items;
            a.retention_days = retention_days;
            a.apply_limits(max_items, retention_days, now)
        })
    }
}

impl ClipRecorder for HistoryService {
    /// 保存の唯一の入口。空白・サイズ・重複・上限の判定をここで行う。
    fn record(&self, new: NewClip) -> Result<RecordOutcome, AppError> {
        self.with_active(|a, now| {
            let NewClip {
                kind,
                content,
                source_app,
            } = new;
            let outcome = match content {
                ClipContent::Text(text) => a.record_text(kind, source_app, text, now)?,
                ClipContent::Image { png, thumb_png } => {
                    a.record_image(source_app, png, thumb_png, now)?
                }
            };
            if matches!(outcome, RecordOutcome::Added(_)) {
                // 追加は成功しているため、上限処理の失敗は記録の失敗にしない(次の機会に再試行される)
                let _ = a.apply_limits(a.max_items, a.retention_days, now);
            }
            Ok(outcome)
        })
    }
}

impl ClipProvider for HistoryService {
    fn get_content(&self, id: i64) -> Result<ClipContent, AppError> {
        self.with_active(|a, _| {
            let e = &a.entries[a.index_of(id)?];
            if e.is_image() {
                let png = a.repo.load_blob(id)?.ok_or(AppError::NotFound)?;
                let thumb_png = e
                    .thumb_b64
                    .as_deref()
                    .and_then(base64_decode)
                    .unwrap_or_default();
                Ok(ClipContent::Image { png, thumb_png })
            } else {
                Ok(ClipContent::Text(e.text.clone().unwrap_or_default()))
            }
        })
    }

    fn promote(&self, id: i64) -> Result<(), AppError> {
        self.with_active(|a, now| {
            let i = a.index_of(id)?;
            a.repo.update_copied_at(id, now)?;
            a.entries[i].copied_at_ms = now;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::cipher::Cipher;
    use crate::crypto::keystore::fake::FakeKeyStore;
    use crate::domain::clip::{AppRef, ClipKind};
    use crate::history::clock::FixedClock;
    use crate::storage::repo::{base64_encode, test_support::TempDir};
    use rusqlite::Connection;

    const T0: i64 = 1_800_000_000_000;
    const DAY: i64 = 86_400_000;
    const KEY: [u8; 32] = [5u8; 32];

    fn mem_service(clock: &Arc<FixedClock>) -> HistoryService {
        let conn = Connection::open_in_memory().unwrap();
        schema::apply_pragmas(&conn).unwrap();
        let repo = Repo::new(conn, Cipher::new(KEY));
        repo.init_new().unwrap();
        HistoryService::from_repo(repo, clock.clone()).unwrap()
    }

    fn setup() -> (Arc<FixedClock>, HistoryService) {
        let clock = FixedClock::new(T0);
        let svc = mem_service(&clock);
        (clock, svc)
    }

    fn text(t: &str) -> NewClip {
        NewClip {
            kind: ClipKind::Text,
            content: ClipContent::Text(t.to_string()),
            source_app: None,
        }
    }

    fn image(png: Vec<u8>) -> NewClip {
        NewClip {
            kind: ClipKind::Image,
            content: ClipContent::Image {
                png,
                thumb_png: vec![1, 2, 3],
            },
            source_app: None,
        }
    }

    fn added(o: RecordOutcome) -> i64 {
        match o {
            RecordOutcome::Added(id) => id,
            other => panic!("Added を期待したが {other:?}"),
        }
    }

    fn texts(svc: &HistoryService, q: &str) -> Vec<String> {
        svc.search(q)
            .unwrap()
            .items
            .into_iter()
            .map(|i| i.first_line)
            .collect()
    }

    // ---- 記録(US-001) ----

    #[test]
    fn テキストを記録すると追加される() {
        let (_c, svc) = setup();
        let id = added(svc.record(text("hello")).unwrap());
        let r = svc.search("").unwrap();
        assert_eq!(r.total, 1);
        assert_eq!(r.items[0].id, id);
        assert_eq!(r.items[0].first_line, "hello");
        assert_eq!(r.items[0].copied_at_ms, T0);
        assert_eq!(r.items[0].char_count, 5);
        assert!(!r.items[0].pinned);
    }

    #[test]
    fn 空文字と空白のみは追加しない() {
        let (_c, svc) = setup();
        for t in ["", " ", " \t\r\n　"] {
            assert_eq!(
                svc.record(text(t)).unwrap(),
                RecordOutcome::Ignored(IgnoreReason::Blank),
                "{t:?}"
            );
        }
        assert_eq!(svc.search("").unwrap().total, 0);
    }

    #[test]
    fn ゼロ幅文字のみは追加しない() {
        let (_c, svc) = setup();
        for z in ['\u{200B}', '\u{200C}', '\u{200D}', '\u{2060}', '\u{FEFF}'] {
            let t = format!(" {z}{z} ");
            assert_eq!(
                svc.record(text(&t)).unwrap(),
                RecordOutcome::Ignored(IgnoreReason::Blank)
            );
        }
        assert_eq!(svc.search("").unwrap().total, 0);
    }

    #[test]
    fn 前後に空白がある本文はそのまま保存する() {
        let (_c, svc) = setup();
        let id = added(svc.record(text("  a  ")).unwrap());
        assert_eq!(svc.detail(id).unwrap().text.as_deref(), Some("  a  "));
    }

    #[test]
    fn ちょうど1MBは追加し1バイト超は追加しない() {
        let (_c, svc) = setup();
        let exact = "a".repeat(1_048_576);
        assert!(matches!(
            svc.record(text(&exact)).unwrap(),
            RecordOutcome::Added(_)
        ));
        let over = "b".repeat(1_048_577);
        assert_eq!(
            svc.record(text(&over)).unwrap(),
            RecordOutcome::Ignored(IgnoreReason::TooLarge)
        );
        assert_eq!(svc.search("").unwrap().total, 1);
    }

    #[test]
    fn サイズはUTF8のバイト数で判定する() {
        let (_c, svc) = setup();
        // "あ" は 3 バイト。349,525 文字 = 1,048,575 バイト(上限内)、349,526 文字 = 1,048,578 バイト(超過)
        assert!(matches!(
            svc.record(text(&"あ".repeat(349_525))).unwrap(),
            RecordOutcome::Added(_)
        ));
        assert_eq!(
            svc.record(text(&"い".repeat(349_526))).unwrap(),
            RecordOutcome::Ignored(IgnoreReason::TooLarge)
        );
    }

    #[test]
    fn 同一内容は新規作成せず先頭へ繰り上げる() {
        let (clock, svc) = setup();
        let a = added(svc.record(text("A")).unwrap());
        clock.advance(1000);
        let b = added(svc.record(text("B")).unwrap());
        clock.advance(1000);
        assert_eq!(svc.record(text("A")).unwrap(), RecordOutcome::Promoted(a));
        let r = svc.search("").unwrap();
        assert_eq!(r.total, 2);
        assert_eq!(r.items.iter().map(|i| i.id).collect::<Vec<_>>(), vec![a, b]);
        assert_eq!(r.items[0].copied_at_ms, T0 + 2000);
    }

    #[test]
    fn 繰り上げた時刻も永続化される() {
        let dir = TempDir::new();
        let clock = FixedClock::new(T0);
        let keys = FakeKeyStore::default();
        let (svc, st, _) = HistoryService::open(&dir.db_path(), &keys, clock.clone());
        assert_eq!(st, StorageStatus::Ok);
        let a = added(svc.record(text("A")).unwrap());
        clock.advance(500);
        svc.record(text("A")).unwrap();
        drop(svc);
        let (svc, _, _) = HistoryService::open(&dir.db_path(), &keys, clock);
        let r = svc.search("").unwrap();
        assert_eq!(
            (r.total, r.items[0].id, r.items[0].copied_at_ms),
            (1, a, T0 + 500)
        );
    }

    #[test]
    fn 記録した履歴は新しいインスタンスで読み直しても残る() {
        let dir = TempDir::new();
        let clock = FixedClock::new(T0);
        let keys = FakeKeyStore::default();
        let (svc, _, _) = HistoryService::open(&dir.db_path(), &keys, clock.clone());
        let app = AppRef {
            id: "com.example".into(),
            name: "Example".into(),
        };
        let mut n = text("persist me");
        n.source_app = Some(app);
        let id = added(svc.record(n).unwrap());
        svc.set_pinned(id, true).unwrap();
        drop(svc);
        let (svc, st, msg) = HistoryService::open(&dir.db_path(), &keys, clock);
        assert_eq!((st, msg), (StorageStatus::Ok, None));
        let r = svc.search("").unwrap();
        assert_eq!(r.total, 1);
        assert_eq!(r.items[0].first_line, "persist me");
        assert_eq!(r.items[0].source_app_name.as_deref(), Some("Example"));
        assert!(r.items[0].pinned);
    }

    #[test]
    fn 保存ファイルに本文が平文で残らない() {
        let dir = TempDir::new();
        let keys = FakeKeyStore::default();
        let (svc, _, _) = HistoryService::open(&dir.db_path(), &keys, FixedClock::new(T0));
        svc.record(text("plain-dummy-secret-text")).unwrap();
        drop(svc);
        let bytes = std::fs::read(dir.db_path()).unwrap();
        assert!(!bytes.windows(12).any(|w| w == b"plain-dummy-"));
    }

    // ---- 上限(US-005・US-011) ----

    #[test]
    fn 千一件目で最古を削除する() {
        let (clock, svc) = setup();
        for i in 0..1001 {
            clock.advance(1);
            svc.record(text(&format!("item-{i}"))).unwrap();
        }
        let r = svc.search("").unwrap();
        assert_eq!(r.total, 1000);
        assert!(texts(&svc, "item-0").iter().all(|t| t != "item-0"));
        assert_eq!(r.items.last().unwrap().first_line, "item-1");
        assert_eq!(r.items[0].first_line, "item-1000");
    }

    #[test]
    fn 件数上限の変更を記録時に反映する() {
        let (clock, svc) = setup();
        svc.set_limits(2, 30);
        for t in ["a", "b", "c"] {
            clock.advance(1);
            svc.record(text(t)).unwrap();
        }
        assert_eq!(texts(&svc, ""), vec!["c", "b"]);
    }

    #[test]
    fn 保存期間ちょうどは残り1ミリ秒超過で削除する() {
        let (clock, svc) = setup();
        svc.record(text("old")).unwrap();
        clock.set(T0 + 30 * DAY);
        assert_eq!(svc.count_over_limits(1000, 30).unwrap(), 0);
        assert_eq!(svc.apply_limits(1000, 30).unwrap(), 0);
        assert_eq!(svc.search("").unwrap().total, 1);
        clock.set(T0 + 30 * DAY + 1);
        assert_eq!(svc.count_over_limits(1000, 30).unwrap(), 1);
        assert_eq!(svc.apply_limits(1000, 30).unwrap(), 1);
        assert_eq!(svc.search("").unwrap().total, 0);
    }

    #[test]
    fn 記録時に期間を過ぎた履歴が自動で削除される() {
        let (clock, svc) = setup();
        svc.set_limits(1000, 1);
        svc.record(text("old")).unwrap();
        clock.advance(2 * DAY);
        svc.record(text("new")).unwrap();
        assert_eq!(texts(&svc, ""), vec!["new"]);
    }

    #[test]
    fn 時計が戻って未来になった履歴は期間で消さない() {
        let (clock, svc) = setup();
        svc.record(text("future")).unwrap();
        clock.set(T0 - 40 * DAY);
        assert_eq!(svc.count_over_limits(1000, 30).unwrap(), 0);
        assert_eq!(svc.apply_limits(1000, 30).unwrap(), 0);
        assert_eq!(svc.search("").unwrap().total, 1);
    }

    #[test]
    fn ピン留めは件数と期間の削除対象にならない() {
        let (clock, svc) = setup();
        let pinned = added(svc.record(text("pinned")).unwrap());
        svc.set_pinned(pinned, true).unwrap();
        clock.advance(1);
        svc.record(text("b")).unwrap();
        clock.advance(1);
        svc.record(text("c")).unwrap();
        clock.advance(100 * DAY);
        assert_eq!(svc.count_over_limits(1, 30).unwrap(), 2);
        assert_eq!(svc.apply_limits(1, 30).unwrap(), 2);
        assert_eq!(texts(&svc, ""), vec!["pinned"]);
    }

    #[test]
    fn 件数上限は新しい順に残して古いものから削除する() {
        let (clock, svc) = setup();
        for t in ["a", "b", "c", "d"] {
            clock.advance(1);
            svc.record(text(t)).unwrap();
        }
        assert_eq!(svc.count_over_limits(2, 30).unwrap(), 2);
        assert_eq!(svc.search("").unwrap().total, 4, "count は削除しない");
        assert_eq!(svc.apply_limits(2, 30).unwrap(), 2);
        assert_eq!(texts(&svc, ""), vec!["d", "c"]);
    }

    // ---- 検索(US-002) ----

    #[test]
    fn 部分一致で検索できる() {
        let (_c, svc) = setup();
        svc.record(text("hello world")).unwrap();
        svc.record(text("goodbye")).unwrap();
        svc.record(text("日本語のテキスト")).unwrap();
        assert_eq!(texts(&svc, "lo wo"), vec!["hello world"]);
        assert_eq!(texts(&svc, "テキ"), vec!["日本語のテキスト"]);
        assert!(texts(&svc, "zzz").is_empty());
    }

    #[test]
    fn 英字の大文字小文字を区別せず検索できる() {
        let (_c, svc) = setup();
        svc.record(text("Hello World")).unwrap();
        assert_eq!(texts(&svc, "HELLO"), vec!["Hello World"]);
        assert_eq!(texts(&svc, "world"), vec!["Hello World"]);
    }

    #[test]
    fn 検索は改行をまたいだ本文全体を対象にする() {
        let (_c, svc) = setup();
        svc.record(text("line1\nsecret-word")).unwrap();
        let r = svc.search("secret-word").unwrap();
        assert_eq!(r.items.len(), 1);
        assert_eq!(r.items[0].first_line, "line1");
    }

    #[test]
    fn 空のクエリは全件を返し_totalは検索条件に関わらない() {
        let (_c, svc) = setup();
        svc.record(text("a1")).unwrap();
        svc.record(text("b2")).unwrap();
        assert_eq!(svc.search("").unwrap().items.len(), 2);
        let r = svc.search("a1").unwrap();
        assert_eq!((r.items.len(), r.total), (1, 2));
    }

    #[test]
    fn ピン留めが先頭で残りは新しい順に並ぶ() {
        let (clock, svc) = setup();
        let mut ids = vec![];
        for t in ["a", "b", "c", "d"] {
            clock.advance(10);
            ids.push(added(svc.record(text(t)).unwrap()));
        }
        svc.set_pinned(ids[0], true).unwrap();
        svc.set_pinned(ids[2], true).unwrap();
        assert_eq!(texts(&svc, ""), vec!["c", "a", "d", "b"]);
    }

    #[test]
    fn 千件各1KBでも検索できる() {
        let (clock, svc) = setup();
        for i in 0..1000 {
            clock.advance(1);
            svc.record(text(&format!("{i:04}-{}", "x".repeat(1024))))
                .unwrap();
        }
        let r = svc.search("0999-").unwrap();
        assert_eq!((r.total, r.items.len()), (1000, 1));
        assert_eq!(svc.search("").unwrap().items.len(), 1000);
    }

    /// release ビルドでの計測用: `cargo test --release -- --ignored`
    #[test]
    #[ignore]
    fn 千件各1KBの検索が200ms以内に終わる() {
        let (clock, svc) = setup();
        for i in 0..1000 {
            clock.advance(1);
            svc.record(text(&format!("{i:04}-{}", "x".repeat(1024))))
                .unwrap();
        }
        let start = std::time::Instant::now();
        let r = svc.search("XXXX").unwrap();
        let elapsed = start.elapsed();
        assert_eq!(r.items.len(), 1000);
        assert!(elapsed.as_millis() < 200, "{elapsed:?}");
    }

    // ---- 削除・ピン留め・貼り付け用 ----

    #[test]
    fn 一件を削除できる_存在しないidはNotFound() {
        let (_c, svc) = setup();
        let id = added(svc.record(text("a")).unwrap());
        svc.delete(id).unwrap();
        assert_eq!(svc.search("").unwrap().total, 0);
        assert_eq!(svc.delete(id), Err(AppError::NotFound));
    }

    #[test]
    fn ピン留めした履歴も一件ずつ削除できる() {
        let (_c, svc) = setup();
        let id = added(svc.record(text("a")).unwrap());
        svc.set_pinned(id, true).unwrap();
        svc.delete(id).unwrap();
        assert_eq!(svc.search("").unwrap().total, 0);
    }

    #[test]
    fn 全削除は削除した件数を返す() {
        let (_c, svc) = setup();
        let id = added(svc.record(text("a")).unwrap());
        svc.record(text("b")).unwrap();
        svc.set_pinned(id, true).unwrap();
        assert_eq!(svc.clear().unwrap(), 2);
        assert_eq!(svc.search("").unwrap().total, 0);
        assert_eq!(svc.clear().unwrap(), 0);
    }

    #[test]
    fn ピン留めを設定して解除できる() {
        let (_c, svc) = setup();
        let id = added(svc.record(text("a")).unwrap());
        svc.set_pinned(id, true).unwrap();
        assert!(svc.search("").unwrap().items[0].pinned);
        svc.set_pinned(id, false).unwrap();
        assert!(!svc.search("").unwrap().items[0].pinned);
        assert_eq!(svc.set_pinned(id + 99, true), Err(AppError::NotFound));
    }

    #[test]
    fn 貼り付け用に内容を取り出せる() {
        let (_c, svc) = setup();
        let id = added(svc.record(text("paste me")).unwrap());
        assert_eq!(
            svc.get_content(id).unwrap(),
            ClipContent::Text("paste me".into())
        );
        assert_eq!(svc.get_content(id + 1), Err(AppError::NotFound));
    }

    #[test]
    fn 貼り付け後の繰り上げで先頭に移り重複しない() {
        let (clock, svc) = setup();
        let a = added(svc.record(text("A")).unwrap());
        clock.advance(100);
        svc.record(text("B")).unwrap();
        clock.advance(100);
        svc.promote(a).unwrap();
        let r = svc.search("").unwrap();
        assert_eq!(
            (r.total, r.items[0].id, r.items[0].copied_at_ms),
            (2, a, T0 + 200)
        );
        assert_eq!(svc.promote(a + 99), Err(AppError::NotFound));
    }

    #[test]
    fn 詳細で全文と文字数を取得できる() {
        let (_c, svc) = setup();
        let id = added(svc.record(text("あいう\nえお")).unwrap());
        let d = svc.detail(id).unwrap();
        assert_eq!(d.text.as_deref(), Some("あいう\nえお"));
        assert_eq!(d.char_count, 6);
        assert_eq!(d.kind, ClipKind::Text);
        assert!(d.image_png.is_none());
        assert_eq!(svc.detail(id + 1), Err(AppError::NotFound));
    }

    // ---- 画像(US-012) ----

    #[test]
    fn 画像を記録しサムネイルと本体を取得できる() {
        let (_c, svc) = setup();
        let png = vec![0x89, b'P', b'N', b'G', 9, 9, 9];
        let id = added(svc.record(image(png.clone())).unwrap());
        let r = svc.search("").unwrap();
        assert_eq!(r.items[0].kind, ClipKind::Image);
        assert_eq!(
            r.items[0].thumb_png.as_deref(),
            Some(base64_encode(&[1, 2, 3]).as_str())
        );
        let d = svc.detail(id).unwrap();
        assert_eq!(d.image_png.as_deref(), Some(base64_encode(&png).as_str()));
        assert!(d.text.is_none());
        assert_eq!(
            svc.get_content(id).unwrap(),
            ClipContent::Image {
                png,
                thumb_png: vec![1, 2, 3]
            }
        );
    }

    #[test]
    fn 画像は10MBちょうどなら追加し超えたら追加しない() {
        let (_c, svc) = setup();
        let mut exact = vec![7u8; 10_485_760];
        exact[0] = 1;
        assert!(matches!(
            svc.record(image(exact)).unwrap(),
            RecordOutcome::Added(_)
        ));
        let over = vec![8u8; 10_485_761];
        assert_eq!(
            svc.record(image(over)).unwrap(),
            RecordOutcome::Ignored(IgnoreReason::TooLarge)
        );
        assert_eq!(svc.search("").unwrap().total, 1);
    }

    #[test]
    fn 同じ画像は繰り上げる() {
        let (clock, svc) = setup();
        let a = added(svc.record(image(vec![1, 1, 1])).unwrap());
        clock.advance(10);
        svc.record(text("t")).unwrap();
        clock.advance(10);
        assert_eq!(
            svc.record(image(vec![1, 1, 1])).unwrap(),
            RecordOutcome::Promoted(a)
        );
        assert_eq!(svc.search("").unwrap().total, 2);
        assert!(matches!(
            svc.record(image(vec![2, 2, 2])).unwrap(),
            RecordOutcome::Added(_)
        ));
    }

    #[test]
    fn 空の画像は追加しない() {
        let (_c, svc) = setup();
        assert_eq!(
            svc.record(image(vec![])).unwrap(),
            RecordOutcome::Ignored(IgnoreReason::Blank)
        );
    }

    #[test]
    fn 画像も検索語があると一致せず_上限と削除の対象になる() {
        let (clock, svc) = setup();
        let id = added(svc.record(image(vec![1, 2])).unwrap());
        assert!(texts(&svc, "a").is_empty());
        clock.advance(31 * DAY);
        assert_eq!(svc.apply_limits(1000, 30).unwrap(), 1);
        assert_eq!(svc.get_content(id), Err(AppError::NotFound));
        let id2 = added(svc.record(image(vec![3, 4])).unwrap());
        svc.delete(id2).unwrap();
        assert_eq!(svc.search("").unwrap().total, 0);
    }

    #[test]
    fn 画像の永続化後も本体を復号できる() {
        let dir = TempDir::new();
        let clock = FixedClock::new(T0);
        let keys = FakeKeyStore::default();
        let (svc, _, _) = HistoryService::open(&dir.db_path(), &keys, clock.clone());
        let id = added(svc.record(image(vec![5, 6, 7])).unwrap());
        drop(svc);
        let (svc, _, _) = HistoryService::open(&dir.db_path(), &keys, clock);
        assert_eq!(
            svc.detail(id).unwrap().image_png.as_deref(),
            Some(base64_encode(&[5, 6, 7]).as_str())
        );
    }

    // ---- 起動時の状態判定(US-006) ----

    #[test]
    fn dbも鍵も無いとき鍵を生成してdbを作る() {
        let dir = TempDir::new();
        let keys = FakeKeyStore::default();
        let (svc, st, msg) = HistoryService::open(&dir.db_path(), &keys, FixedClock::new(T0));
        assert_eq!((st, msg), (StorageStatus::Ok, None));
        assert!(keys.key.lock().unwrap().is_some());
        assert_eq!(*keys.save_count.lock().unwrap(), 1);
        assert!(dir.db_path().exists());
        svc.record(text("a")).unwrap();
    }

    #[test]
    fn 親ディレクトリが無くてもdbを作れる() {
        let dir = TempDir::new();
        let path = dir.0.join("sub").join("deep").join("nuebako.db");
        let (_svc, st, _) =
            HistoryService::open(&path, &FakeKeyStore::default(), FixedClock::new(T0));
        assert_eq!(st, StorageStatus::Ok);
        assert!(path.exists());
    }

    #[test]
    fn dbが無く鍵があるとき既存の鍵でdbを作る() {
        let dir = TempDir::new();
        let keys = FakeKeyStore::with_key(KEY);
        let (_svc, st, _) = HistoryService::open(&dir.db_path(), &keys, FixedClock::new(T0));
        assert_eq!(st, StorageStatus::Ok);
        assert_eq!(*keys.save_count.lock().unwrap(), 0);
        assert_eq!(*keys.key.lock().unwrap(), Some(KEY));
    }

    #[test]
    fn dbが無く鍵の取得に失敗したとき鍵もdbも作らない() {
        let dir = TempDir::new();
        let keys = FakeKeyStore::failing();
        let (svc, st, msg) = HistoryService::open(&dir.db_path(), &keys, FixedClock::new(T0));
        assert_eq!(st, StorageStatus::KeyUnavailable);
        assert!(msg.is_some());
        assert!(!dir.db_path().exists());
        assert_eq!(*keys.save_count.lock().unwrap(), 0);
        assert_eq!(svc.search("").err(), Some(AppError::KeyUnavailable));
    }

    fn make_db(dir: &TempDir, keys: &FakeKeyStore) -> Vec<u8> {
        let (svc, _, _) = HistoryService::open(&dir.db_path(), keys, FixedClock::new(T0));
        svc.record(text("existing-dummy")).unwrap();
        drop(svc);
        std::fs::read(dir.db_path()).unwrap()
    }

    fn file_names(dir: &TempDir) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(&dir.0)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn dbと鍵があり鍵が正しいとき履歴を読み込む() {
        let dir = TempDir::new();
        let keys = FakeKeyStore::default();
        make_db(&dir, &keys);
        let (svc, st, msg) = HistoryService::open(&dir.db_path(), &keys, FixedClock::new(T0));
        assert_eq!((st, msg), (StorageStatus::Ok, None));
        assert_eq!(texts(&svc, ""), vec!["existing-dummy"]);
    }

    #[test]
    fn dbがあり鍵の取得に失敗したとき読み込まず書き込まず鍵も作らない() {
        let dir = TempDir::new();
        let keys = FakeKeyStore::default();
        let before = make_db(&dir, &keys);
        let files_before = file_names(&dir);
        let failing = FakeKeyStore::failing();
        let (svc, st, msg) = HistoryService::open(&dir.db_path(), &failing, FixedClock::new(T0));
        assert_eq!(st, StorageStatus::KeyUnavailable);
        assert!(msg.unwrap().contains("暗号鍵"));
        assert_eq!(std::fs::read(dir.db_path()).unwrap(), before);
        assert_eq!(file_names(&dir), files_before);
        assert_eq!(*failing.save_count.lock().unwrap(), 0);
        assert!(failing.key.lock().unwrap().is_none());
        // 全メソッドが KeyUnavailable を返す
        assert_eq!(svc.search("").err(), Some(AppError::KeyUnavailable));
        assert_eq!(svc.record(text("x")), Err(AppError::KeyUnavailable));
        assert_eq!(svc.detail(1), Err(AppError::KeyUnavailable));
        assert_eq!(svc.delete(1), Err(AppError::KeyUnavailable));
        assert_eq!(svc.clear(), Err(AppError::KeyUnavailable));
        assert_eq!(svc.set_pinned(1, true), Err(AppError::KeyUnavailable));
        assert_eq!(svc.count_over_limits(10, 10), Err(AppError::KeyUnavailable));
        assert_eq!(svc.apply_limits(10, 10), Err(AppError::KeyUnavailable));
        assert_eq!(svc.get_content(1), Err(AppError::KeyUnavailable));
        assert_eq!(svc.promote(1), Err(AppError::KeyUnavailable));
        assert_eq!(std::fs::read(dir.db_path()).unwrap(), before);
    }

    #[test]
    fn dbがあり鍵が無いとき鍵を作らずdbも変更しない() {
        let dir = TempDir::new();
        let keys = FakeKeyStore::default();
        let before = make_db(&dir, &keys);
        let empty = FakeKeyStore::default();
        let (svc, st, _) = HistoryService::open(&dir.db_path(), &empty, FixedClock::new(T0));
        assert_eq!(st, StorageStatus::KeyUnavailable);
        assert!(empty.key.lock().unwrap().is_none());
        assert_eq!(*empty.save_count.lock().unwrap(), 0);
        assert_eq!(std::fs::read(dir.db_path()).unwrap(), before);
        assert_eq!(svc.record(text("x")), Err(AppError::KeyUnavailable));
    }

    #[test]
    fn dbがあり鍵が違うときKeyMismatchで読み込まずdbも変更しない() {
        let dir = TempDir::new();
        let keys = FakeKeyStore::default();
        let before = make_db(&dir, &keys);
        let other = FakeKeyStore::with_key([9u8; 32]);
        let (svc, st, msg) = HistoryService::open(&dir.db_path(), &other, FixedClock::new(T0));
        assert_eq!(st, StorageStatus::KeyMismatch);
        assert!(msg.is_some());
        assert_eq!(svc.search("").err(), Some(AppError::KeyMismatch));
        assert_eq!(svc.record(text("x")), Err(AppError::KeyMismatch));
        assert_eq!(*other.save_count.lock().unwrap(), 0);
        drop(svc);
        assert_eq!(std::fs::read(dir.db_path()).unwrap(), before);
    }

    #[test]
    fn 復号できない行は読み飛ばし件数だけ数え行は消さない() {
        let dir = TempDir::new();
        let keys = FakeKeyStore::default();
        let (svc, _, _) = HistoryService::open(&dir.db_path(), &keys, FixedClock::new(T0));
        let good = added(svc.record(text("good")).unwrap());
        let bad = added(svc.record(text("bad")).unwrap());
        drop(svc);
        {
            let conn = Connection::open(dir.db_path()).unwrap();
            conn.execute(
                "UPDATE clips SET payload = X'00112233445566778899AABBCCDDEEFF00112233445566778899AABBCCDDEEFF' WHERE id=?1",
                [bad],
            )
            .unwrap();
        }
        let (svc, st, _) = HistoryService::open(&dir.db_path(), &keys, FixedClock::new(T0));
        assert_eq!(st, StorageStatus::Ok);
        assert_eq!(svc.skipped_count(), 1);
        let r = svc.search("").unwrap();
        assert_eq!((r.total, r.items[0].id), (1, good));
        drop(svc);
        let conn = Connection::open(dir.db_path()).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM clips", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn 新しい版数のdbはErrorにして変更しない() {
        let dir = TempDir::new();
        {
            let conn = Connection::open(dir.db_path()).unwrap();
            conn.execute_batch("PRAGMA user_version = 99; CREATE TABLE x(a);")
                .unwrap();
        }
        let before = std::fs::read(dir.db_path()).unwrap();
        let keys = FakeKeyStore::with_key(KEY);
        let (svc, st, msg) = HistoryService::open(&dir.db_path(), &keys, FixedClock::new(T0));
        assert_eq!(st, StorageStatus::Error);
        assert!(msg.is_some());
        assert!(matches!(svc.search("").err(), Some(AppError::Storage(_))));
        drop(svc);
        assert_eq!(std::fs::read(dir.db_path()).unwrap(), before);
    }

    #[test]
    fn 壊れたdbファイルはErrorにして変更しない() {
        let dir = TempDir::new();
        std::fs::write(
            dir.db_path(),
            b"this is not a sqlite database at all, just dummy bytes....",
        )
        .unwrap();
        let before = std::fs::read(dir.db_path()).unwrap();
        let keys = FakeKeyStore::with_key(KEY);
        let (_svc, st, _) = HistoryService::open(&dir.db_path(), &keys, FixedClock::new(T0));
        assert_eq!(st, StorageStatus::Error);
        assert_eq!(std::fs::read(dir.db_path()).unwrap(), before);
    }

    #[test]
    fn 空のdbファイルは新規として初期化する() {
        let dir = TempDir::new();
        std::fs::write(dir.db_path(), b"").unwrap();
        let keys = FakeKeyStore::with_key(KEY);
        let (svc, st, _) = HistoryService::open(&dir.db_path(), &keys, FixedClock::new(T0));
        assert_eq!(st, StorageStatus::Ok);
        svc.record(text("a")).unwrap();
    }
}
