//! スキーマの作成と版数(`PRAGMA user_version`)。

use rusqlite::Connection;

use crate::error::AppError;

/// 現在のスキーマ版数(`PRAGMA user_version`)。
pub const SCHEMA_VERSION: i64 = 1;

/// rusqlite のエラーを `AppError::Storage` にする。履歴の内容は含まれない。
pub fn db_err(e: rusqlite::Error) -> AppError {
    AppError::Storage(e.to_string())
}

/// 接続時の設定(WAL、外部キー、削除時の上書き)。
pub fn apply_pragmas(conn: &Connection) -> Result<(), AppError> {
    // journal_mode は結果の行を返すため query_row で実行する(メモリ DB では "memory" が返る)
    conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get::<_, String>(0))
        .map_err(db_err)?;
    conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA secure_delete=ON;")
        .map_err(db_err)
}

/// `PRAGMA user_version` を返す。
pub fn user_version(conn: &Connection) -> Result<i64, AppError> {
    conn.query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(db_err)
}

/// テーブルとインデックスを作り、鍵の確認用データ(`key_check`)を書き、`user_version` を設定する。
/// すべてを1つのトランザクションで行う。途中で止まったときに、「スキーマはあるが `key_check` が無い」
/// 状態が残ると、次の起動で鍵を確かめられず、恒久的に開けなくなるため。
pub fn create_schema(conn: &Connection, key_check: &[u8]) -> Result<(), AppError> {
    conn.execute_batch("BEGIN").map_err(db_err)?;
    let result = (|| {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS meta (
                 key TEXT PRIMARY KEY,
                 value BLOB NOT NULL
             );
             CREATE TABLE IF NOT EXISTS clips (
                 id INTEGER PRIMARY KEY,
                 copied_at INTEGER NOT NULL,
                 payload BLOB NOT NULL
             );
             CREATE TABLE IF NOT EXISTS clip_blobs (
                 clip_id INTEGER PRIMARY KEY REFERENCES clips(id) ON DELETE CASCADE,
                 payload BLOB NOT NULL
             );
             CREATE INDEX IF NOT EXISTS clips_copied_at ON clips(copied_at);",
        )?;
        write_key_check(conn, key_check)?;
        conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))
    })();
    match result {
        Ok(()) => conn.execute_batch("COMMIT").map_err(db_err),
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(db_err(e))
        }
    }
}

/// `meta` に `key_check` を書く(あれば置き換える)。
pub fn write_key_check(conn: &Connection, key_check: &[u8]) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('key_check', ?1)",
        [key_check],
    )
    .map(|_| ())
}
