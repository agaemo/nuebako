//! 履歴サービス。保存の入口(record)、検索、削除、上限処理、起動時の鍵と DB の状態判定を担う。
//!
//! 全件をメモリに持ち(本文は復号済み)、検索と重複判定はメモリ上で行う。
//! 画像本体だけは必要になるまで DB から復号して読む。
//! 時刻の保存は UTC の Unix ミリ秒。
//!
//! 重い処理(10MB の復号、base64 化、ハッシュ)は、ロックを持たない所で行う。
//! ロックを持ったまま行うと、その間の検索や記録がすべて待たされるため。

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use super::clock::Clock;
use super::hash::fnv1a_64;
use super::search::{self, fold, is_blank, preview};
use super::{ClipProvider, ClipRecorder, IgnoreReason, MAX_IMAGE_BYTES, RecordOutcome};
use crate::crypto::cipher::Cipher;
use crate::crypto::keystore::{KeyStore, load_or_create};
use crate::domain::clip::{AppRef, ClipContent, ClipKind, NewClip};
use crate::domain::dto::{ClipDetail, ClipSummary, StorageStatus};
use crate::domain::settings::{DEFAULT_MAX_ITEMS, DEFAULT_RETENTION_DAYS};
use crate::error::AppError;
use crate::storage::repo::{Payload, Repo, StoredRow, base64_decode, base64_encode};
use crate::storage::{perms, schema};
use rusqlite::Connection;

/// テキスト1件の上限(UTF-8 で 1 MiB)。
const MAX_TEXT_BYTES: usize = 1_048_576;
const DAY_MS: i64 = 86_400_000;
/// 画像の行に表示する先頭行。
const IMAGE_LABEL: &str = "(画像)";

/// 検索結果(`offset` から `limit` 件分)。
pub struct SearchResult {
    /// 履歴の全件数(検索条件に関わらない)
    pub total: u32,
    /// 検索に一致した全件数(`items` はこのうちの一部)
    pub matched: u32,
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
            Some(t) => (fold(t), fnv1a_64(t.as_bytes()), t.chars().count() as u32),
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
        let (first_line, extra_lines) = match &self.text {
            Some(t) => {
                let p = preview(t);
                (p.first_line, p.extra_lines)
            }
            None => (IMAGE_LABEL.to_string(), 0),
        };
        ClipSummary {
            id: self.id,
            kind: self.kind,
            first_line,
            extra_lines,
            copied_at_ms: self.copied_at_ms,
            pinned: self.pinned,
            source_app_name: self.source_app.as_ref().map(|a| a.name.clone()),
            char_count: self.char_count,
            thumb_png: self.thumb_b64.clone(),
        }
    }
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

/// 上限(件数・保存期間)を適用したときの削除の計画。
struct LimitPlan {
    /// 削除する id
    ids: Vec<i64>,
    /// 保存期間を過ぎているが、時計の異常を疑って削除を見送った件数
    held_back: u32,
}

/// 上限(件数・保存期間)を適用すると削除される id を求める。ピン留めは対象外。
///
/// 保存期間は「経過時間が保存期間を超えた」もの(ちょうどは残す)。未来の時刻は消さない。
/// 期間による削除は、期間内の記録が1件以上あるときだけ行う。全件が期間外に見えるのは、
/// 時計が大きく進んだときの可能性が高く、そのまま消すと履歴が全損するため。
/// 件数はピン留めを除いた件数で数え、新しい順に残す。
fn plan_limits(entries: &[Entry], max_items: u32, retention_days: u32, now: i64) -> LimitPlan {
    let cutoff = now.saturating_sub(i64::from(retention_days).saturating_mul(DAY_MS));
    let clock_plausible = entries.iter().any(|e| e.copied_at_ms >= cutoff);
    let mut live: Vec<&Entry> = Vec::new();
    let mut ids = Vec::new();
    let mut held_back = 0;
    for e in entries.iter().filter(|e| !e.pinned) {
        if e.copied_at_ms >= cutoff {
            live.push(e);
        } else if clock_plausible {
            ids.push(e.id);
        } else {
            held_back += 1;
            live.push(e);
        }
    }
    live.sort_by(|a, b| b.copied_at_ms.cmp(&a.copied_at_ms).then(b.id.cmp(&a.id)));
    ids.extend(live.iter().skip(max_items as usize).map(|e| e.id));
    LimitPlan { ids, held_back }
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
        let plan = plan_limits(&self.entries, max_items, retention_days, now);
        if plan.held_back > 0 {
            // 内容は出さず、件数だけを残す
            eprintln!(
                "保存期間を過ぎた履歴 {} 件は、時計が進んだ可能性があるため削除しませんでした",
                plan.held_back
            );
        }
        if plan.ids.is_empty() {
            return Ok(0);
        }
        let n = self.repo.delete_ids(&plan.ids)?;
        self.entries.retain(|e| !plan.ids.contains(&e.id));
        Ok(n)
    }

    fn record_text(
        &mut self,
        kind: ClipKind,
        source_app: Option<AppRef>,
        text: String,
        hash: u64,
        now: i64,
    ) -> Result<RecordOutcome, AppError> {
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
        image: PreparedImage,
        now: i64,
    ) -> Result<RecordOutcome, AppError> {
        let PreparedImage {
            png,
            hash,
            thumb_b64,
        } = image;
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
            thumb_png: Some(thumb_b64),
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

/// 記録の前に、ロックの外で済ませておく処理(ハッシュ、base64 化)をした画像。
struct PreparedImage {
    png: Vec<u8>,
    hash: u64,
    thumb_b64: String,
}

/// ロックの外で復号するために、ロックの中で取り出した画像本体の暗号文。
struct SealedBlob {
    encrypted: Vec<u8>,
    cipher: Arc<Cipher>,
}

impl SealedBlob {
    fn open(self, id: i64) -> Result<Vec<u8>, AppError> {
        Repo::decrypt_blob(&self.cipher, id, &self.encrypted)
    }
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
    /// 上限(件数・保存期間)は既定値で始まる。設定を読んだ後に `apply_limits` を呼ぶこと。
    pub fn open(
        db_path: &Path,
        key_store: &dyn KeyStore,
        clock: Arc<dyn Clock>,
    ) -> (Self, StorageStatus, Option<String>) {
        match Self::try_open(db_path, key_store) {
            Ok((active, skipped)) => {
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
        // WAL と共有メモリのファイルは、DB ファイルの権限を引き継いで作られるため、先に絞る
        perms::restrict_db_files(db_path);
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
        perms::restrict_db_files(db_path);
        Active::load(repo)
    }

    /// 開いている `Repo` から作る(読み込み済みの DB 用)。
    #[cfg(test)]
    pub(crate) fn from_repo(repo: Repo, clock: Arc<dyn Clock>) -> Result<Self, AppError> {
        let (active, skipped) = Active::load(repo)?;
        Ok(HistoryService {
            state: Mutex::new(State::Ready(Box::new(active))),
            clock,
            skipped,
        })
    }

    /// 起動時に復号できず読み飛ばした行の数。
    pub fn skipped_count(&self) -> u32 {
        self.skipped
    }

    /// 内部の状態を保持したスレッドがパニックしたことがあるか。
    ///
    /// 履歴は DB が正で、メモリ上の写しは DB の書き込みの後に更新する。そのため、途中でパニックしても
    /// 写しが古くなるだけで、DB は壊れない。動作は続けるが、画面には保存の異常として知らせる。
    pub fn is_poisoned(&self) -> bool {
        self.state.is_poisoned()
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// 利用できる状態のときだけ `f` を呼ぶ。使えない状態では、その理由のエラーを返し DB に触れない。
    fn with_active<T>(
        &self,
        f: impl FnOnce(&mut Active, i64) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut guard = self.lock();
        match &mut *guard {
            State::Ready(a) => f(a, self.clock.now_ms()),
            State::Unavailable(e) => Err(e.clone()),
        }
    }

    /// 履歴を検索する。ピン留めを先頭に、残りは新しい順。空のクエリは全件。
    /// 一致した全件のうち、`offset` 番目から `limit` 件を返す。
    pub fn search(
        &self,
        query: &str,
        limit: usize,
        offset: usize,
    ) -> Result<SearchResult, AppError> {
        let q = fold(query);
        // ロックの中では、必要な件数の写しを作るだけにする
        self.with_active(|a, _| {
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
                matched: hits.len() as u32,
                items: hits
                    .into_iter()
                    .skip(offset)
                    .take(limit)
                    .map(Entry::summary)
                    .collect(),
            })
        })
    }

    pub fn detail(&self, id: i64) -> Result<ClipDetail, AppError> {
        let (detail, sealed) = self.with_active(|a, _| {
            let e = &a.entries[a.index_of(id)?];
            let sealed = if e.is_image() {
                Some(SealedBlob {
                    encrypted: a.repo.load_blob_encrypted(id)?.ok_or(AppError::NotFound)?,
                    cipher: a.repo.cipher(),
                })
            } else {
                None
            };
            let detail = ClipDetail {
                id,
                kind: e.kind,
                text: e.text.clone(),
                image_png: None,
                source_app_name: e.source_app.as_ref().map(|s| s.name.clone()),
                copied_at_ms: e.copied_at_ms,
                char_count: e.char_count,
            };
            Ok((detail, sealed))
        })?;
        // 復号と base64 化は、ロックを離してから行う(最大 10MB)
        let image_png = sealed
            .map(|s| s.open(id).map(|png| base64_encode(&png)))
            .transpose()?;
        Ok(ClipDetail {
            image_png,
            ..detail
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
            Ok(plan_limits(&a.entries, max_items, retention_days, now)
                .ids
                .len() as u32)
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

    /// WAL に残った削除済みの内容を、DB 本体へ書き戻して WAL を空にする(終了時に呼ぶ)。
    pub fn checkpoint(&self) {
        let _ = self.with_active(|a, _| {
            a.repo.checkpoint();
            Ok(())
        });
    }
}

impl ClipRecorder for HistoryService {
    /// 保存の唯一の入口。空白・サイズ・重複・上限の判定をここで行う。
    fn record(&self, new: NewClip) -> Result<RecordOutcome, AppError> {
        let NewClip {
            kind,
            content,
            source_app,
        } = new;
        // 判定とハッシュ・base64 化は、ロックの外で済ませる
        let prepared = match content {
            ClipContent::Text(text) => {
                if is_blank(&text) {
                    return Ok(RecordOutcome::Ignored(IgnoreReason::Blank));
                }
                if text.len() > MAX_TEXT_BYTES {
                    return Ok(RecordOutcome::Ignored(IgnoreReason::TooLarge));
                }
                let hash = fnv1a_64(text.as_bytes());
                Prepared::Text(text, hash)
            }
            ClipContent::Image { png, thumb_png } => {
                if png.is_empty() {
                    return Ok(RecordOutcome::Ignored(IgnoreReason::Blank));
                }
                if png.len() > MAX_IMAGE_BYTES {
                    return Ok(RecordOutcome::Ignored(IgnoreReason::TooLarge));
                }
                Prepared::Image(PreparedImage {
                    hash: fnv1a_64(&png),
                    thumb_b64: base64_encode(&thumb_png),
                    png,
                })
            }
        };
        self.with_active(|a, now| {
            let outcome = match prepared {
                Prepared::Text(text, hash) => a.record_text(kind, source_app, text, hash, now)?,
                Prepared::Image(image) => a.record_image(source_app, image, now)?,
            };
            if matches!(outcome, RecordOutcome::Added(_)) {
                // 追加は成功しているため、上限処理の失敗は記録の失敗にしない(次の機会に再試行される)
                let _ = a.apply_limits(a.max_items, a.retention_days, now);
            }
            Ok(outcome)
        })
    }
}

enum Prepared {
    Text(String, u64),
    Image(PreparedImage),
}

impl ClipProvider for HistoryService {
    fn get_content(&self, id: i64) -> Result<ClipContent, AppError> {
        enum Taken {
            Text(String),
            Image {
                sealed: SealedBlob,
                thumb_b64: Option<String>,
            },
        }
        let taken = self.with_active(|a, _| {
            let e = &a.entries[a.index_of(id)?];
            if e.is_image() {
                Ok(Taken::Image {
                    sealed: SealedBlob {
                        encrypted: a.repo.load_blob_encrypted(id)?.ok_or(AppError::NotFound)?,
                        cipher: a.repo.cipher(),
                    },
                    thumb_b64: e.thumb_b64.clone(),
                })
            } else {
                Ok(Taken::Text(e.text.clone().unwrap_or_default()))
            }
        })?;
        match taken {
            Taken::Text(t) => Ok(ClipContent::Text(t)),
            Taken::Image { sealed, thumb_b64 } => {
                let png = sealed.open(id)?;
                let thumb_png = thumb_b64
                    .as_deref()
                    .and_then(base64_decode)
                    .unwrap_or_default();
                Ok(ClipContent::Image { png, thumb_png })
            }
        }
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
    use std::panic::{AssertUnwindSafe, catch_unwind};

    const T0: i64 = 1_800_000_000_000;
    const DAY: i64 = 86_400_000;
    const KEY: [u8; 32] = [5u8; 32];
    /// ページ分けをしない検索(全件)
    const ALL: usize = 100_000;

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
        svc.search(q, ALL, 0)
            .unwrap()
            .items
            .into_iter()
            .map(|i| i.first_line)
            .collect()
    }

    // ---- 記録 ----

    #[test]
    fn テキストを記録すると追加される() {
        let (_c, svc) = setup();
        let id = added(svc.record(text("hello")).unwrap());
        let r = svc.search("", ALL, 0).unwrap();
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
        assert_eq!(svc.search("", ALL, 0).unwrap().total, 0);
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
        assert_eq!(svc.search("", ALL, 0).unwrap().total, 0);
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
        assert_eq!(svc.search("", ALL, 0).unwrap().total, 1);
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
        let r = svc.search("", ALL, 0).unwrap();
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
        let r = svc.search("", ALL, 0).unwrap();
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
        let r = svc.search("", ALL, 0).unwrap();
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

    // ---- 上限 ----

    #[test]
    fn 千一件目で最古を削除する() {
        let (clock, svc) = setup();
        for i in 0..1001 {
            clock.advance(1);
            svc.record(text(&format!("item-{i}"))).unwrap();
        }
        let r = svc.search("", ALL, 0).unwrap();
        assert_eq!(r.total, 1000);
        assert!(texts(&svc, "item-0").iter().all(|t| t != "item-0"));
        assert_eq!(r.items.last().unwrap().first_line, "item-1");
        assert_eq!(r.items[0].first_line, "item-1000");
    }

    #[test]
    fn 件数上限の変更を記録時に反映する() {
        let (clock, svc) = setup();
        svc.apply_limits(2, 30).unwrap();
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
        // 期間内の記録が1件あれば、期間による削除が働く(全件が期間外のときは別のテストで確認する)
        clock.set(T0 + 30 * DAY);
        svc.record(text("recent")).unwrap();
        assert_eq!(svc.count_over_limits(1000, 30).unwrap(), 0);
        assert_eq!(svc.apply_limits(1000, 30).unwrap(), 0);
        assert_eq!(svc.search("", ALL, 0).unwrap().total, 2);
        clock.set(T0 + 30 * DAY + 1);
        assert_eq!(svc.count_over_limits(1000, 30).unwrap(), 1);
        assert_eq!(svc.apply_limits(1000, 30).unwrap(), 1);
        assert_eq!(texts(&svc, ""), vec!["recent"]);
    }

    #[test]
    fn 全件が保存期間外のとき時計が進んだとみなして削除しない() {
        let (clock, svc) = setup();
        svc.record(text("a")).unwrap();
        clock.advance(1);
        svc.record(text("b")).unwrap();
        // 時計が1年進んだ状態。期間内の記録が1件も無いので、期間では消さない
        clock.set(T0 + 365 * DAY);
        assert_eq!(svc.count_over_limits(1000, 30).unwrap(), 0);
        assert_eq!(svc.apply_limits(1000, 30).unwrap(), 0);
        assert_eq!(svc.search("", ALL, 0).unwrap().total, 2);
    }

    #[test]
    fn 全件が保存期間外でも件数の上限は適用する() {
        let (clock, svc) = setup();
        for t in ["a", "b", "c"] {
            clock.advance(1);
            svc.record(text(t)).unwrap();
        }
        clock.set(T0 + 365 * DAY);
        assert_eq!(svc.apply_limits(2, 30).unwrap(), 1);
        assert_eq!(texts(&svc, ""), vec!["c", "b"]);
    }

    #[test]
    fn ピン留めだけが期間内なら時計は正常とみなして古い履歴を削除する() {
        let (clock, svc) = setup();
        // 記録のたびに自動で削除されないよう、保存期間を長くしておく
        svc.apply_limits(1000, 3650).unwrap();
        svc.record(text("old")).unwrap();
        clock.set(T0 + 100 * DAY);
        let pinned = added(svc.record(text("pinned")).unwrap());
        svc.set_pinned(pinned, true).unwrap();
        assert_eq!(svc.apply_limits(1000, 30).unwrap(), 1);
        assert_eq!(texts(&svc, ""), vec!["pinned"]);
    }

    #[test]
    fn 記録時に期間を過ぎた履歴が自動で削除される() {
        let (clock, svc) = setup();
        svc.apply_limits(1000, 1).unwrap();
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
        assert_eq!(svc.search("", ALL, 0).unwrap().total, 1);
    }

    #[test]
    fn ピン留めは件数と期間の削除対象にならない() {
        let (clock, svc) = setup();
        // 記録のたびに自動で削除されないよう、保存期間を長くしておく
        svc.apply_limits(1000, 3650).unwrap();
        let pinned = added(svc.record(text("pinned")).unwrap());
        svc.set_pinned(pinned, true).unwrap();
        clock.advance(1);
        svc.record(text("b")).unwrap();
        clock.advance(1);
        svc.record(text("c")).unwrap();
        clock.advance(100 * DAY);
        svc.record(text("d")).unwrap();
        // b・c は期間外、d は件数の上限(1)の内側。ピン留めはどちらでも消えない
        assert_eq!(svc.count_over_limits(1, 30).unwrap(), 2);
        assert_eq!(svc.apply_limits(1, 30).unwrap(), 2);
        assert_eq!(texts(&svc, ""), vec!["pinned", "d"]);
    }

    #[test]
    fn 件数上限は新しい順に残して古いものから削除する() {
        let (clock, svc) = setup();
        for t in ["a", "b", "c", "d"] {
            clock.advance(1);
            svc.record(text(t)).unwrap();
        }
        assert_eq!(svc.count_over_limits(2, 30).unwrap(), 2);
        assert_eq!(
            svc.search("", ALL, 0).unwrap().total,
            4,
            "count は削除しない"
        );
        assert_eq!(svc.apply_limits(2, 30).unwrap(), 2);
        assert_eq!(texts(&svc, ""), vec!["d", "c"]);
    }

    // ---- 検索 ----

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
        let r = svc.search("secret-word", ALL, 0).unwrap();
        assert_eq!(r.items.len(), 1);
        assert_eq!(r.items[0].first_line, "line1");
    }

    #[test]
    fn 空のクエリは全件を返し_totalは検索条件に関わらない() {
        let (_c, svc) = setup();
        svc.record(text("a1")).unwrap();
        svc.record(text("b2")).unwrap();
        assert_eq!(svc.search("", ALL, 0).unwrap().items.len(), 2);
        let r = svc.search("a1", ALL, 0).unwrap();
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

    /// 1KB の本文を 1,000 件入れる(検索の速度の確認用)。
    fn fill_1000(clock: &FixedClock, svc: &HistoryService) {
        for i in 0..1000 {
            clock.advance(1);
            svc.record(text(&format!("{i:04}-{}", "x".repeat(1024))))
                .unwrap();
        }
    }

    #[test]
    fn 千件各1KBでも検索できる() {
        let (clock, svc) = setup();
        fill_1000(&clock, &svc);
        let r = svc.search("0999-", ALL, 0).unwrap();
        assert_eq!((r.total, r.items.len()), (1000, 1));
        assert_eq!(svc.search("", ALL, 0).unwrap().items.len(), 1000);
    }

    /// release ビルドでの計測用: `cargo test --release -- --ignored`
    #[test]
    #[ignore]
    fn 千件各1KBの検索が200ms以内に終わる() {
        let (clock, svc) = setup();
        fill_1000(&clock, &svc);
        let start = std::time::Instant::now();
        let r = svc.search("XXXX", ALL, 0).unwrap();
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
        assert_eq!(svc.search("", ALL, 0).unwrap().total, 0);
        assert_eq!(svc.delete(id), Err(AppError::NotFound));
    }

    #[test]
    fn ピン留めした履歴も一件ずつ削除できる() {
        let (_c, svc) = setup();
        let id = added(svc.record(text("a")).unwrap());
        svc.set_pinned(id, true).unwrap();
        svc.delete(id).unwrap();
        assert_eq!(svc.search("", ALL, 0).unwrap().total, 0);
    }

    #[test]
    fn 全削除は削除した件数を返す() {
        let (_c, svc) = setup();
        let id = added(svc.record(text("a")).unwrap());
        svc.record(text("b")).unwrap();
        svc.set_pinned(id, true).unwrap();
        assert_eq!(svc.clear().unwrap(), 2);
        assert_eq!(svc.search("", ALL, 0).unwrap().total, 0);
        assert_eq!(svc.clear().unwrap(), 0);
    }

    #[test]
    fn ピン留めを設定して解除できる() {
        let (_c, svc) = setup();
        let id = added(svc.record(text("a")).unwrap());
        svc.set_pinned(id, true).unwrap();
        assert!(svc.search("", ALL, 0).unwrap().items[0].pinned);
        svc.set_pinned(id, false).unwrap();
        assert!(!svc.search("", ALL, 0).unwrap().items[0].pinned);
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
        let r = svc.search("", ALL, 0).unwrap();
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

    // ---- 画像 ----

    #[test]
    fn 画像を記録しサムネイルと本体を取得できる() {
        let (_c, svc) = setup();
        let png = vec![0x89, b'P', b'N', b'G', 9, 9, 9];
        let id = added(svc.record(image(png.clone())).unwrap());
        let r = svc.search("", ALL, 0).unwrap();
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
        assert_eq!(svc.search("", ALL, 0).unwrap().total, 1);
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
        assert_eq!(svc.search("", ALL, 0).unwrap().total, 2);
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
        svc.apply_limits(1000, 3650).unwrap();
        let id = added(svc.record(image(vec![1, 2])).unwrap());
        assert!(texts(&svc, "a").is_empty());
        clock.advance(31 * DAY);
        svc.record(text("recent")).unwrap();
        assert_eq!(svc.apply_limits(1000, 30).unwrap(), 1);
        assert_eq!(svc.get_content(id), Err(AppError::NotFound));
        let id2 = added(svc.record(image(vec![3, 4])).unwrap());
        svc.delete(id2).unwrap();
        assert_eq!(texts(&svc, ""), vec!["recent"]);
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

    // ---- 起動時の状態判定 ----

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
        assert_eq!(svc.search("", ALL, 0).err(), Some(AppError::KeyUnavailable));
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
        assert_eq!(svc.search("", ALL, 0).err(), Some(AppError::KeyUnavailable));
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
        assert_eq!(svc.search("", ALL, 0).err(), Some(AppError::KeyMismatch));
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
        let r = svc.search("", ALL, 0).unwrap();
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
        assert!(matches!(
            svc.search("", ALL, 0).err(),
            Some(AppError::Storage(_))
        ));
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

    // ---- 一覧のページ分けと要約 ----

    #[test]
    fn 検索はoffsetからlimit件を返しmatchedは一致の全件数になる() {
        let (clock, svc) = setup();
        for i in 0..10 {
            clock.advance(1);
            svc.record(text(&format!("item-{i}"))).unwrap();
        }
        svc.record(text("other")).unwrap();
        let r = svc.search("item", 4, 3).unwrap();
        assert_eq!((r.total, r.matched, r.items.len()), (11, 10, 4));
        // 新しい順の 4〜7 番目(item-6 から item-3)
        assert_eq!(r.items[0].first_line, "item-6");
        assert_eq!(r.items[3].first_line, "item-3");
        let r = svc.search("item", 4, 8).unwrap();
        assert_eq!((r.matched, r.items.len()), (10, 2));
        let r = svc.search("item", 4, 50).unwrap();
        assert_eq!((r.matched, r.items.len()), (10, 0));
    }

    #[test]
    fn 要約は先頭の非空の行と追加の行数を返す() {
        let (_c, svc) = setup();
        svc.record(text("\n \nfoo\nbar\n\nbaz\n")).unwrap();
        let r = svc.search("", ALL, 0).unwrap();
        assert_eq!(r.items[0].first_line, "foo");
        assert_eq!(r.items[0].extra_lines, 3);
    }

    #[test]
    fn 画像の要約は追加の行数が0でサムネイルを返す() {
        let (_c, svc) = setup();
        svc.record(image(vec![1, 2, 3])).unwrap();
        let r = svc.search("", ALL, 0).unwrap();
        assert_eq!(r.items[0].extra_lines, 0);
        assert!(r.items[0].thumb_png.is_some());
    }

    // ---- 画像のハッシュ ----

    #[test]
    fn 画像の重複判定用ハッシュは固定のアルゴリズムで保存される() {
        let dir = TempDir::new();
        let keys = FakeKeyStore::with_key(KEY);
        let clock = FixedClock::new(T0);
        let (svc, _, _) = HistoryService::open(&dir.db_path(), &keys, clock);
        let png = vec![9u8, 8, 7, 6];
        svc.record(image(png.clone())).unwrap();
        drop(svc);
        let repo = Repo::new(Connection::open(dir.db_path()).unwrap(), Cipher::new(KEY));
        let rows = repo.load_all().unwrap().0;
        // FNV-1a 64bit の値(版や実行をまたいでも変わらない)
        assert_eq!(rows[0].payload.blob_hash, Some(0xf248_a2ba_2083_584d));
    }

    // ---- 内部状態の異常 ----

    #[test]
    fn 内部状態を持つスレッドがパニックしても動作を続けpoisonedを返す() {
        let (_c, svc) = setup();
        svc.record(text("before")).unwrap();
        assert!(!svc.is_poisoned());
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let _guard = svc.state.lock().unwrap();
            panic!("テスト用のパニック");
        }));
        assert!(svc.is_poisoned());
        // DB が正なので、動作は続けられる
        assert_eq!(texts(&svc, ""), vec!["before"]);
        svc.record(text("after")).unwrap();
        assert_eq!(svc.search("", ALL, 0).unwrap().total, 2);
    }

    // ---- 削除後の WAL ----

    fn wal_len(dir: &TempDir) -> u64 {
        std::fs::metadata(dir.0.join("nuebako.db-wal"))
            .map(|m| m.len())
            .unwrap_or(0)
    }

    #[test]
    fn 削除したあとWALに内容が残らない() {
        let dir = TempDir::new();
        let (svc, _, _) = HistoryService::open(
            &dir.db_path(),
            &FakeKeyStore::with_key(KEY),
            FixedClock::new(T0),
        );
        let id = added(svc.record(text(&"x".repeat(5000))).unwrap());
        assert!(wal_len(&dir) > 0, "削除の前は WAL に書き込みがある");
        svc.delete(id).unwrap();
        assert_eq!(wal_len(&dir), 0, "単体削除のあと");
        svc.record(text(&"y".repeat(5000))).unwrap();
        svc.clear().unwrap();
        assert_eq!(wal_len(&dir), 0, "全削除のあと");
    }

    #[test]
    fn 上限による削除のあともWALに内容が残らない() {
        let dir = TempDir::new();
        let clock = FixedClock::new(T0);
        let (svc, _, _) =
            HistoryService::open(&dir.db_path(), &FakeKeyStore::with_key(KEY), clock.clone());
        for t in ["a", "b", "c"] {
            clock.advance(1);
            svc.record(text(&format!("{t}{}", "x".repeat(5000))))
                .unwrap();
        }
        assert_eq!(svc.apply_limits(1, 30).unwrap(), 2);
        assert_eq!(wal_len(&dir), 0);
    }

    #[test]
    fn 終了時のcheckpointでWALが空になる() {
        let dir = TempDir::new();
        let (svc, _, _) = HistoryService::open(
            &dir.db_path(),
            &FakeKeyStore::with_key(KEY),
            FixedClock::new(T0),
        );
        svc.record(text(&"x".repeat(5000))).unwrap();
        assert!(wal_len(&dir) > 0);
        svc.checkpoint();
        assert_eq!(wal_len(&dir), 0);
    }

    // ---- ファイルの権限 ----

    #[cfg(unix)]
    #[test]
    fn 保存ファイルは所有者だけが読み書きできる() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new();
        let path = dir.0.join("data");
        let db = path.join("nuebako.db");
        let (svc, st, _) =
            HistoryService::open(&db, &FakeKeyStore::with_key(KEY), FixedClock::new(T0));
        assert_eq!(st, StorageStatus::Ok);
        svc.record(text("a")).unwrap();
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o700);
        assert_eq!(mode(&db), 0o600);
        assert_eq!(mode(&path.join("nuebako.db-wal")), 0o600);
        assert_eq!(mode(&path.join("nuebako.db-shm")), 0o600);
    }

    // ---- key_check の自己修復 ----

    fn delete_key_check(dir: &TempDir) {
        let conn = Connection::open(dir.db_path()).unwrap();
        conn.execute("DELETE FROM meta WHERE key='key_check'", [])
            .unwrap();
    }

    #[test]
    fn key_checkが無く履歴も空のdbは作り直して開ける() {
        let dir = TempDir::new();
        let keys = FakeKeyStore::with_key(KEY);
        let (svc, _, _) = HistoryService::open(&dir.db_path(), &keys, FixedClock::new(T0));
        drop(svc);
        delete_key_check(&dir);
        let (svc, st, msg) = HistoryService::open(&dir.db_path(), &keys, FixedClock::new(T0));
        assert_eq!((st, msg), (StorageStatus::Ok, None));
        svc.record(text("a")).unwrap();
        drop(svc);
        // 修復した key_check で、次の起動も開ける。違う鍵は拒否される
        let (_svc, st, _) = HistoryService::open(&dir.db_path(), &keys, FixedClock::new(T0));
        assert_eq!(st, StorageStatus::Ok);
        let other = FakeKeyStore::with_key([9u8; 32]);
        let (_svc, st, _) = HistoryService::open(&dir.db_path(), &other, FixedClock::new(T0));
        assert_eq!(st, StorageStatus::KeyMismatch);
    }

    #[test]
    fn key_checkが無く履歴がある場合は読み込まず書き込まない() {
        let dir = TempDir::new();
        let keys = FakeKeyStore::with_key(KEY);
        let (svc, _, _) = HistoryService::open(&dir.db_path(), &keys, FixedClock::new(T0));
        svc.record(text("existing-dummy")).unwrap();
        drop(svc);
        delete_key_check(&dir);
        let before = std::fs::read(dir.db_path()).unwrap();
        let (svc, st, msg) = HistoryService::open(&dir.db_path(), &keys, FixedClock::new(T0));
        assert_eq!(st, StorageStatus::Error);
        assert!(msg.is_some());
        assert!(matches!(svc.record(text("x")), Err(AppError::Storage(_))));
        drop(svc);
        assert_eq!(std::fs::read(dir.db_path()).unwrap(), before);
    }
}
