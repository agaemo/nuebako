//! リポジトリ(SQLite)。1件ずつ暗号化して保存する。

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::schema;
use crate::crypto::cipher::Cipher;
use crate::domain::clip::{AppRef, ClipKind};
use crate::error::AppError;

/// 鍵の確認に使う既知の文字列。
pub const KEY_CHECK_PLAIN: &[u8] = b"nuebako-key-check-v1";

/// `clips.payload` の平文(暗号化前)。JSON で保存する。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payload {
    pub v: u32,
    pub kind: ClipKind,
    pub text: Option<String>,
    pub source_app: Option<AppRef>,
    pub pinned: bool,
    /// サムネイル PNG の base64
    pub thumb_png: Option<String>,
    /// 画像本体のハッシュ(重複判定用。暗号化された payload の中にだけ置く)
    pub blob_hash: Option<u64>,
}

impl Payload {
    pub fn text(kind: ClipKind, text: &str, source_app: Option<AppRef>) -> Self {
        Self {
            v: 1,
            kind,
            text: Some(text.to_string()),
            source_app,
            pinned: false,
            thumb_png: None,
            blob_hash: None,
        }
    }
}

/// 復号した1行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredRow {
    pub id: i64,
    pub copied_at_ms: i64,
    pub payload: Payload,
}

pub struct Repo {
    conn: Connection,
    cipher: Cipher,
}

/// 履歴本体の AAD。行の入れ替えを検出するため id を含める。
fn clip_aad(id: i64) -> Vec<u8> {
    format!("nuebako:clips:v1:{id}").into_bytes()
}

/// 画像本体の AAD。
fn blob_aad(id: i64) -> Vec<u8> {
    format!("nuebako:blobs:v1:{id}").into_bytes()
}

const KEY_CHECK_AAD: &[u8] = b"nuebako:meta:v1:key_check";

use schema::db_err;

impl Repo {
    pub fn new(conn: Connection, cipher: Cipher) -> Self {
        Self { conn, cipher }
    }

    /// 接続の設定(WAL など)を適用する。
    pub fn apply_pragmas(&self) -> Result<(), AppError> {
        schema::apply_pragmas(&self.conn)
    }

    /// ユーザーのテーブル数(新規 DB かの判定用)。
    pub fn table_count(&self) -> Result<i64, AppError> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
                [],
                |r| r.get(0),
            )
            .map_err(db_err)
    }

    /// 新しい DB にスキーマと key_check を作る。
    pub fn init_new(&self) -> Result<(), AppError> {
        schema::create_schema(&self.conn)?;
        let enc = self.cipher.encrypt(KEY_CHECK_AAD, KEY_CHECK_PLAIN)?;
        self.conn
            .execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('key_check', ?1)",
                [enc],
            )
            .map_err(db_err)?;
        Ok(())
    }

    /// key_check を復号して鍵を確かめる。復号できなければ `KeyMismatch`。
    /// 行が無い(または meta テーブルが無い)場合は、確認できないため `Storage`。
    pub fn check_key(&self) -> Result<(), AppError> {
        let row: Option<Vec<u8>> =
            match self
                .conn
                .query_row("SELECT value FROM meta WHERE key='key_check'", [], |r| {
                    r.get(0)
                }) {
                Ok(v) => Some(v),
                Err(rusqlite::Error::QueryReturnedNoRows) => None,
                Err(e) => return Err(db_err(e)),
            };
        let enc = row.ok_or_else(|| AppError::Storage("鍵の確認情報がありません".to_string()))?;
        match self.cipher.decrypt(KEY_CHECK_AAD, &enc) {
            Ok(plain) if plain == KEY_CHECK_PLAIN => Ok(()),
            _ => Err(AppError::KeyMismatch),
        }
    }

    /// 1件追加して id を返す。id は AAD に使うため、行を作ってから暗号化して書き込む(1トランザクション)。
    pub fn insert(
        &mut self,
        copied_at_ms: i64,
        payload: &Payload,
        blob: Option<&[u8]>,
    ) -> Result<i64, AppError> {
        let json = serde_json::to_vec(payload).map_err(|e| AppError::Storage(e.to_string()))?;
        let tx = self.conn.transaction().map_err(db_err)?;
        tx.execute(
            "INSERT INTO clips (copied_at, payload) VALUES (?1, ?2)",
            rusqlite::params![copied_at_ms, Vec::<u8>::new()],
        )
        .map_err(db_err)?;
        let id = tx.last_insert_rowid();
        let enc = self.cipher.encrypt(&clip_aad(id), &json)?;
        tx.execute(
            "UPDATE clips SET payload=?1 WHERE id=?2",
            rusqlite::params![enc, id],
        )
        .map_err(db_err)?;
        if let Some(blob) = blob {
            let enc = self.cipher.encrypt(&blob_aad(id), blob)?;
            tx.execute(
                "INSERT INTO clip_blobs (clip_id, payload) VALUES (?1, ?2)",
                rusqlite::params![id, enc],
            )
            .map_err(db_err)?;
        }
        tx.commit().map_err(db_err)?;
        Ok(id)
    }

    pub fn update_copied_at(&self, id: i64, copied_at_ms: i64) -> Result<bool, AppError> {
        let n = self
            .conn
            .execute(
                "UPDATE clips SET copied_at=?1 WHERE id=?2",
                rusqlite::params![copied_at_ms, id],
            )
            .map_err(db_err)?;
        Ok(n > 0)
    }

    pub fn update_payload(&self, id: i64, payload: &Payload) -> Result<bool, AppError> {
        let json = serde_json::to_vec(payload).map_err(|e| AppError::Storage(e.to_string()))?;
        let enc = self.cipher.encrypt(&clip_aad(id), &json)?;
        let n = self
            .conn
            .execute(
                "UPDATE clips SET payload=?1 WHERE id=?2",
                rusqlite::params![enc, id],
            )
            .map_err(db_err)?;
        Ok(n > 0)
    }

    pub fn delete(&self, id: i64) -> Result<bool, AppError> {
        let n = self
            .conn
            .execute("DELETE FROM clips WHERE id=?1", [id])
            .map_err(db_err)?;
        Ok(n > 0)
    }

    /// 複数の id をまとめて削除し、実際に消えた件数を返す(1トランザクション)。
    pub fn delete_ids(&mut self, ids: &[i64]) -> Result<u32, AppError> {
        let tx = self.conn.transaction().map_err(db_err)?;
        let mut n = 0u32;
        {
            let mut stmt = tx
                .prepare("DELETE FROM clips WHERE id=?1")
                .map_err(db_err)?;
            for id in ids {
                n += stmt.execute([id]).map_err(db_err)? as u32;
            }
        }
        tx.commit().map_err(db_err)?;
        Ok(n)
    }

    pub fn delete_all(&self) -> Result<u32, AppError> {
        let n = self.conn.execute("DELETE FROM clips", []).map_err(db_err)?;
        Ok(n as u32)
    }

    /// 全行を復号して返す。復号できない行は読み飛ばし、その件数を返す(行は消さない)。
    pub fn load_all(&self) -> Result<(Vec<StoredRow>, u32), AppError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, copied_at, payload FROM clips ORDER BY id")
            .map_err(db_err)?;
        let raw = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, Vec<u8>>(2)?,
                ))
            })
            .map_err(db_err)?;
        let mut rows = Vec::new();
        let mut skipped = 0u32;
        for item in raw {
            let (id, copied_at_ms, enc) = item.map_err(db_err)?;
            let payload = self
                .cipher
                .decrypt(&clip_aad(id), &enc)
                .ok()
                .and_then(|plain| serde_json::from_slice::<Payload>(&plain).ok());
            match payload {
                Some(payload) => rows.push(StoredRow {
                    id,
                    copied_at_ms,
                    payload,
                }),
                None => skipped += 1,
            }
        }
        Ok((rows, skipped))
    }

    pub fn load_blob(&self, id: i64) -> Result<Option<Vec<u8>>, AppError> {
        let enc: Vec<u8> = match self.conn.query_row(
            "SELECT payload FROM clip_blobs WHERE clip_id=?1",
            [id],
            |r| r.get(0),
        ) {
            Ok(v) => v,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
            Err(e) => return Err(db_err(e)),
        };
        self.cipher.decrypt(&blob_aad(id), &enc).map(Some)
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// base64(標準、パディングあり)にする。
pub fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            B64[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// base64 を戻す。不正な入力は `None`。
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let bytes = s.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let val = |c: u8| B64.iter().position(|&x| x == c).map(|p| p as u32);
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let last = bytes.len() / 4;
    for (i, quad) in bytes.chunks(4).enumerate() {
        let pad = quad.iter().rev().take_while(|&&c| c == b'=').count();
        if pad > 2 || (pad > 0 && i + 1 != last) {
            return None;
        }
        let mut n = 0u32;
        for (j, &c) in quad.iter().enumerate() {
            let v = if j >= 4 - pad { 0 } else { val(c)? };
            n = (n << 6) | v;
        }
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// テスト用の一時ディレクトリ。Drop で削除する。
    pub struct TempDir(pub PathBuf);

    impl TempDir {
        pub fn new() -> Self {
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let dir = std::env::temp_dir().join(format!("nuebako-test-{}-{n}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }
        pub fn db_path(&self) -> PathBuf {
            self.0.join("nuebako.db")
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::TempDir;
    use super::*;

    const KEY: [u8; 32] = [1u8; 32];

    fn repo() -> Repo {
        let conn = Connection::open_in_memory().unwrap();
        schema::apply_pragmas(&conn).unwrap();
        let r = Repo::new(conn, Cipher::new(KEY));
        r.init_new().unwrap();
        r
    }

    fn pl(t: &str) -> Payload {
        Payload::text(ClipKind::Text, t, None)
    }

    fn raw_count(r: &Repo, table: &str) -> i64 {
        r.conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap()
    }

    #[test]
    fn スキーマを作るとuser_versionが1になる() {
        let r = repo();
        assert_eq!(schema::user_version(&r.conn).unwrap(), 1);
    }

    #[test]
    fn 新しい接続は外部キーとsecure_deleteが有効() {
        let r = repo();
        let fk: i64 = r
            .conn
            .query_row("PRAGMA foreign_keys", [], |x| x.get(0))
            .unwrap();
        let sd: i64 = r
            .conn
            .query_row("PRAGMA secure_delete", [], |x| x.get(0))
            .unwrap();
        assert_eq!(fk, 1);
        assert_eq!(sd, 1);
    }

    #[test]
    fn ファイルのDBはWALで開く() {
        let dir = TempDir::new();
        let conn = Connection::open(dir.db_path()).unwrap();
        schema::apply_pragmas(&conn).unwrap();
        let mode: String = conn
            .query_row("PRAGMA journal_mode", [], |x| x.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
    }

    #[test]
    fn 挿入した行を読み出せる() {
        let mut r = repo();
        let id = r.insert(1000, &pl("ダミー"), None).unwrap();
        let (rows, skipped) = r.load_all().unwrap();
        assert_eq!(skipped, 0);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, id);
        assert_eq!(rows[0].copied_at_ms, 1000);
        assert_eq!(rows[0].payload, pl("ダミー"));
    }

    #[test]
    fn 挿入するたびにidが異なる() {
        let mut r = repo();
        let a = r.insert(1, &pl("a"), None).unwrap();
        let b = r.insert(2, &pl("b"), None).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn copied_atを更新できる() {
        let mut r = repo();
        let id = r.insert(1, &pl("a"), None).unwrap();
        assert!(r.update_copied_at(id, 99).unwrap());
        assert!(!r.update_copied_at(id + 100, 99).unwrap());
        assert_eq!(r.load_all().unwrap().0[0].copied_at_ms, 99);
    }

    #[test]
    fn payloadを更新しても同じidで復号できる() {
        let mut r = repo();
        let id = r.insert(1, &pl("a"), None).unwrap();
        let mut p = pl("a");
        p.pinned = true;
        assert!(r.update_payload(id, &p).unwrap());
        let rows = r.load_all().unwrap().0;
        assert!(rows[0].payload.pinned);
    }

    #[test]
    fn 一件を削除できる() {
        let mut r = repo();
        let id = r.insert(1, &pl("a"), None).unwrap();
        assert!(r.delete(id).unwrap());
        assert!(!r.delete(id).unwrap());
        assert_eq!(raw_count(&r, "clips"), 0);
    }

    #[test]
    fn 複数のidをまとめて削除できる() {
        let mut r = repo();
        let a = r.insert(1, &pl("a"), None).unwrap();
        let b = r.insert(2, &pl("b"), None).unwrap();
        let _c = r.insert(3, &pl("c"), None).unwrap();
        assert_eq!(r.delete_ids(&[a, b, 9999]).unwrap(), 2);
        assert_eq!(raw_count(&r, "clips"), 1);
    }

    #[test]
    fn 全削除は件数を返す() {
        let mut r = repo();
        r.insert(1, &pl("a"), None).unwrap();
        r.insert(2, &pl("b"), None).unwrap();
        assert_eq!(r.delete_all().unwrap(), 2);
        assert_eq!(raw_count(&r, "clips"), 0);
    }

    #[test]
    fn 画像本体を暗号化して保存し読み出せる() {
        let mut r = repo();
        let png = b"\x89PNG-dummy-image-bytes".to_vec();
        let id = r.insert(1, &pl("x"), Some(&png)).unwrap();
        assert_eq!(r.load_blob(id).unwrap(), Some(png.clone()));
        let raw: Vec<u8> = r
            .conn
            .query_row(
                "SELECT payload FROM clip_blobs WHERE clip_id=?1",
                [id],
                |x| x.get(0),
            )
            .unwrap();
        assert!(!raw.windows(8).any(|w| w == &png[..8]));
        assert_eq!(r.load_blob(id + 1).unwrap(), None);
    }

    #[test]
    fn クリップを削除すると画像本体もカスケードで消える() {
        let mut r = repo();
        let id = r.insert(1, &pl("x"), Some(b"png")).unwrap();
        r.delete(id).unwrap();
        assert_eq!(raw_count(&r, "clip_blobs"), 0);
        let id2 = r.insert(2, &pl("y"), Some(b"png")).unwrap();
        let _ = id2;
        r.delete_all().unwrap();
        assert_eq!(raw_count(&r, "clip_blobs"), 0);
    }

    #[test]
    fn 保存されたpayloadに平文が含まれない() {
        let mut r = repo();
        r.insert(1, &pl("plain-dummy-secret"), None).unwrap();
        let raw: Vec<u8> = r
            .conn
            .query_row("SELECT payload FROM clips", [], |x| x.get(0))
            .unwrap();
        assert!(!raw.windows(12).any(|w| w == b"plain-dummy-"));
    }

    #[test]
    fn 行を入れ替えると復号できず読み飛ばし行は消えない() {
        let mut r = repo();
        let a = r.insert(1, &pl("a"), None).unwrap();
        let b = r.insert(2, &pl("b"), None).unwrap();
        let pa: Vec<u8> = r
            .conn
            .query_row("SELECT payload FROM clips WHERE id=?1", [a], |x| x.get(0))
            .unwrap();
        let pb: Vec<u8> = r
            .conn
            .query_row("SELECT payload FROM clips WHERE id=?1", [b], |x| x.get(0))
            .unwrap();
        r.conn
            .execute(
                "UPDATE clips SET payload=?1 WHERE id=?2",
                rusqlite::params![pb, a],
            )
            .unwrap();
        r.conn
            .execute(
                "UPDATE clips SET payload=?1 WHERE id=?2",
                rusqlite::params![pa, b],
            )
            .unwrap();
        let (rows, skipped) = r.load_all().unwrap();
        assert_eq!(rows.len(), 0);
        assert_eq!(skipped, 2);
        assert_eq!(raw_count(&r, "clips"), 2);
    }

    #[test]
    fn 違う鍵では全行を読み飛ばし行は消えない() {
        let mut r = repo();
        r.insert(1, &pl("a"), None).unwrap();
        r.insert(2, &pl("b"), None).unwrap();
        let other = Repo::new(r.conn, Cipher::new([2u8; 32]));
        let (rows, skipped) = other.load_all().unwrap();
        assert_eq!((rows.len(), skipped), (0, 2));
        assert_eq!(raw_count(&other, "clips"), 2);
    }

    #[test]
    fn SQL文のような文字列も安全に保存できる() {
        let mut r = repo();
        let evil = "'); DROP TABLE clips;--";
        r.insert(1, &pl(evil), None).unwrap();
        let (rows, _) = r.load_all().unwrap();
        assert_eq!(rows[0].payload.text.as_deref(), Some(evil));
        assert_eq!(raw_count(&r, "clips"), 1);
    }

    #[test]
    fn 正しい鍵ならkey_checkが通る() {
        assert!(repo().check_key().is_ok());
    }

    #[test]
    fn 違う鍵ならkey_checkがKeyMismatchになる() {
        let r = repo();
        let other = Repo::new(r.conn, Cipher::new([2u8; 32]));
        assert_eq!(other.check_key(), Err(AppError::KeyMismatch));
    }

    #[test]
    fn key_checkの行が無いときはStorageエラーになる() {
        let r = repo();
        r.conn.execute("DELETE FROM meta", []).unwrap();
        assert!(matches!(r.check_key(), Err(AppError::Storage(_))));
    }

    #[test]
    fn base64はRFC4648の例と一致する() {
        let cases: [(&[u8], &str); 5] = [
            (b"", ""),
            (b"f", "Zg=="),
            (b"fo", "Zm8="),
            (b"foo", "Zm9v"),
            (b"foobar", "Zm9vYmFy"),
        ];
        for (raw, enc) in cases {
            assert_eq!(base64_encode(raw), enc);
            assert_eq!(base64_decode(enc).as_deref(), Some(raw));
        }
        assert_eq!(base64_decode("!!!!"), None);
        assert_eq!(base64_decode("Zg="), None);
    }

    #[test]
    fn base64は全バイト値で往復できる() {
        let data: Vec<u8> = (0..=255u8).collect();
        assert_eq!(base64_decode(&base64_encode(&data)), Some(data));
    }
}
