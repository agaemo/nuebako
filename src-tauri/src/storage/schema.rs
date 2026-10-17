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

/// テーブルとインデックスを作り、`user_version` を設定する。
pub fn create_schema(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch(&format!(
        "BEGIN;
         CREATE TABLE IF NOT EXISTS meta (
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
         CREATE INDEX IF NOT EXISTS clips_copied_at ON clips(copied_at);
         PRAGMA user_version = {SCHEMA_VERSION};
         COMMIT;"
    ))
    .map_err(db_err)
}
