//! 履歴の検索・詳細・削除・ピン留め。引数の検証と `AppError` への変換だけを行い、
//! 処理は `HistoryService` に任せる。

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};

use crate::app_state::AppState;
use crate::domain::dto::{ClipDetail, ClipPage};
use crate::error::AppError;
use crate::history::HistoryService;

/// 検索語の最大文字数。
const MAX_QUERY_CHARS: usize = 1000;

/// 履歴の変更を画面に知らせるイベント(内容は含めない)。
const EVENT_HISTORY_CHANGED: &str = "history-changed";

/// `AppState` から `HistoryService` を取り出す。未登録ならパニックせずエラーにする。
fn history(app: &AppHandle) -> Result<Arc<HistoryService>, AppError> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| AppError::Internal("アプリの状態が初期化されていません".to_string()))?;
    Ok(Arc::clone(&state.history))
}

/// 履歴の変更を通知する。通知の失敗は操作の失敗にしない。
fn notify_changed(app: &AppHandle) {
    let _ = app.emit(EVENT_HISTORY_CHANGED, ());
}

/// 1回に返す件数の範囲。画面は 50 件ずつ読み込む。
const MIN_LIMIT: u32 = 1;
const MAX_LIMIT: u32 = 200;

/// `limit` を 1〜200 に丸める(0 や極端に大きい値で、IPC の応答が空になったり、巨大になったりしないように)。
fn clamp_limit(limit: u32) -> usize {
    limit.clamp(MIN_LIMIT, MAX_LIMIT) as usize
}

fn list_clips_impl(
    h: &HistoryService,
    query: &str,
    req_id: u64,
    limit: u32,
    offset: u32,
) -> Result<ClipPage, AppError> {
    if query.chars().count() > MAX_QUERY_CHARS {
        return Err(AppError::validation("query", "検索語が長すぎます"));
    }
    let r = h.search(query, clamp_limit(limit), offset as usize)?;
    Ok(ClipPage {
        req_id,
        total: r.total,
        matched: r.matched,
        items: r.items,
    })
}

/// 履歴を検索して、`offset` 番目から `limit` 件を返す。
/// 検索と、サムネイルを含む応答の組み立てがあるため、メインスレッドで動かさない(`async`)。
#[tauri::command(async)]
pub fn list_clips(
    app: AppHandle,
    query: String,
    req_id: u64,
    limit: u32,
    offset: u32,
) -> Result<ClipPage, AppError> {
    let h = history(&app)?;
    list_clips_impl(&h, &query, req_id, limit, offset)
}

/// 画像は復号と base64 化に時間がかかるため、メインスレッドで動かさない(`async`)。
#[tauri::command(async)]
pub fn get_clip_detail(app: AppHandle, id: i64) -> Result<ClipDetail, AppError> {
    history(&app)?.detail(id)
}

#[tauri::command(async)]
pub fn delete_clip(app: AppHandle, id: i64) -> Result<(), AppError> {
    history(&app)?.delete(id)?;
    notify_changed(&app);
    Ok(())
}

/// 全件を削除し、削除した件数を返す。
#[tauri::command(async)]
pub fn clear_history(app: AppHandle) -> Result<u32, AppError> {
    let n = history(&app)?.clear()?;
    notify_changed(&app);
    Ok(n)
}

#[tauri::command(async)]
pub fn set_pinned(app: AppHandle, id: i64, pinned: bool) -> Result<(), AppError> {
    history(&app)?.set_pinned(id, pinned)?;
    notify_changed(&app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::cipher::Cipher;
    use crate::domain::clip::{ClipContent, ClipKind, NewClip};
    use crate::history::ClipRecorder;
    use crate::history::clock::FixedClock;
    use crate::storage::repo::Repo;
    use crate::storage::schema;
    use rusqlite::Connection;

    fn service() -> HistoryService {
        let conn = Connection::open_in_memory().unwrap();
        schema::apply_pragmas(&conn).unwrap();
        let repo = Repo::new(conn, Cipher::new([4u8; 32]));
        repo.init_new().unwrap();
        HistoryService::from_repo(repo, FixedClock::new(1_800_000_000_000)).unwrap()
    }

    fn add(h: &HistoryService, t: &str) {
        h.record(NewClip {
            kind: ClipKind::Text,
            content: ClipContent::Text(t.to_string()),
            source_app: None,
        })
        .unwrap();
    }

    #[test]
    fn req_idをそのまま返す() {
        let h = service();
        add(&h, "a");
        let page = list_clips_impl(&h, "", 12345, 50, 0).unwrap();
        assert_eq!(page.req_id, 12345);
        let page = list_clips_impl(&h, "zzz", u64::MAX, 50, 0).unwrap();
        assert_eq!(
            (page.req_id, page.total, page.matched, page.items.len()),
            (u64::MAX, 1, 0, 0)
        );
    }

    #[test]
    fn limitは1から200に丸める() {
        assert_eq!(clamp_limit(0), 1);
        assert_eq!(clamp_limit(1), 1);
        assert_eq!(clamp_limit(50), 50);
        assert_eq!(clamp_limit(200), 200);
        assert_eq!(clamp_limit(201), 200);
        assert_eq!(clamp_limit(u32::MAX), 200);
    }

    #[test]
    fn offsetからlimit件を返しmatchedは一致した全件数になる() {
        let h = service();
        for i in 0..7 {
            add(&h, &format!("item-{i}"));
        }
        add(&h, "other");
        let page = list_clips_impl(&h, "item", 1, 3, 2).unwrap();
        assert_eq!((page.total, page.matched, page.items.len()), (8, 7, 3));
        let page = list_clips_impl(&h, "item", 1, 3, 6).unwrap();
        assert_eq!((page.matched, page.items.len()), (7, 1));
        let page = list_clips_impl(&h, "item", 1, 3, 100).unwrap();
        assert_eq!((page.matched, page.items.len()), (7, 0));
    }

    #[test]
    fn limitが0でも1件は返す() {
        let h = service();
        add(&h, "a");
        add(&h, "b");
        assert_eq!(list_clips_impl(&h, "", 1, 0, 0).unwrap().items.len(), 1);
    }

    #[test]
    fn 応答のキーはsnake_caseで一致件数と追加の行数を含む() {
        let h = service();
        add(&h, "one\ntwo\nthree");
        let json = serde_json::to_value(list_clips_impl(&h, "", 9, 50, 0).unwrap()).unwrap();
        assert_eq!(json["req_id"], 9);
        assert_eq!(json["total"], 1);
        assert_eq!(json["matched"], 1);
        assert_eq!(json["items"][0]["first_line"], "one");
        assert_eq!(json["items"][0]["extra_lines"], 2);
    }

    #[test]
    fn 長すぎる検索語は検証エラーになる() {
        let h = service();
        let q = "a".repeat(MAX_QUERY_CHARS + 1);
        let err = list_clips_impl(&h, &q, 1, 50, 0).unwrap_err();
        assert_eq!(err.code(), "validation");
    }
}
