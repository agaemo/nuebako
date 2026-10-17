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

fn list_clips_impl(h: &HistoryService, query: &str, req_id: u64) -> Result<ClipPage, AppError> {
    if query.chars().count() > MAX_QUERY_CHARS {
        return Err(AppError::validation("query", "検索語が長すぎます"));
    }
    let r = h.search(query)?;
    Ok(ClipPage {
        req_id,
        total: r.total,
        items: r.items,
    })
}

#[tauri::command]
pub fn list_clips(app: AppHandle, query: String, req_id: u64) -> Result<ClipPage, AppError> {
    let h = history(&app)?;
    list_clips_impl(&h, &query, req_id)
}

#[tauri::command]
pub fn get_clip_detail(app: AppHandle, id: i64) -> Result<ClipDetail, AppError> {
    history(&app)?.detail(id)
}

#[tauri::command]
pub fn delete_clip(app: AppHandle, id: i64) -> Result<(), AppError> {
    history(&app)?.delete(id)?;
    notify_changed(&app);
    Ok(())
}

/// 全件を削除し、削除した件数を返す。
#[tauri::command]
pub fn clear_history(app: AppHandle) -> Result<u32, AppError> {
    let n = history(&app)?.clear()?;
    notify_changed(&app);
    Ok(n)
}

#[tauri::command]
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
        let page = list_clips_impl(&h, "", 12345).unwrap();
        assert_eq!(page.req_id, 12345);
        assert_eq!(page.total, 1);
        let page = list_clips_impl(&h, "zzz", u64::MAX).unwrap();
        assert_eq!(
            (page.req_id, page.total, page.items.len()),
            (u64::MAX, 1, 0)
        );
    }

    #[test]
    fn first_lineは最初の改行までになる() {
        let h = service();
        add(&h, "one\ntwo\nthree");
        let page = list_clips_impl(&h, "", 1).unwrap();
        assert_eq!(page.items[0].first_line, "one");
    }

    #[test]
    fn first_lineは最大200文字でサロゲートペアの途中で切らない() {
        let h = service();
        add(&h, &format!("{}😀{}", "a".repeat(199), "b".repeat(50)));
        let page = list_clips_impl(&h, "", 1).unwrap();
        let line = &page.items[0].first_line;
        assert_eq!(line.chars().count(), 200);
        assert!(line.ends_with('😀'));
        assert!(page.items[0].char_count > 200);
    }

    #[test]
    fn 長すぎる検索語は検証エラーになる() {
        let h = service();
        let q = "a".repeat(MAX_QUERY_CHARS + 1);
        let err = list_clips_impl(&h, &q, 1).unwrap_err();
        assert_eq!(err.code(), "validation");
    }

    #[test]
    fn 存在しないidはnot_foundになる() {
        let h = service();
        assert_eq!(h.detail(999).unwrap_err().code(), "not_found");
        assert_eq!(h.delete(999).unwrap_err().code(), "not_found");
        assert_eq!(h.set_pinned(999, true).unwrap_err().code(), "not_found");
    }
}
