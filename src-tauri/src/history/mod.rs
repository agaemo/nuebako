// 骨組み(P1-4)。本体は各トラックで実装し、P3-1 でこの属性を外す。
#![allow(dead_code, unused_variables)]

//! 履歴の保存と検索(track-store)。

pub mod clock;
pub mod search;
pub mod service;

pub use service::HistoryService;

use crate::domain::clip::{ClipContent, NewClip};
use crate::error::AppError;

/// 記録の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordOutcome {
    /// 新しく追加した(id)
    Added(i64),
    /// 同じ内容があったため、先頭に繰り上げた(id)
    Promoted(i64),
    /// 条件に合わないため記録しなかった
    Ignored(IgnoreReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgnoreReason {
    /// 空文字、空白のみ、ゼロ幅文字のみ
    Blank,
    /// サイズ上限(テキスト 1MB、画像 10MB)を超えた
    TooLarge,
}

/// 履歴の保存口。保存の入口はここだけで、capture は storage を直接呼ばない。
pub trait ClipRecorder: Send + Sync {
    fn record(&self, new: NewClip) -> Result<RecordOutcome, AppError>;
}

/// 貼り付けのために、履歴の内容を取り出す。
pub trait ClipProvider: Send + Sync {
    /// 貼り付ける内容を返す。
    fn get_content(&self, id: i64) -> Result<ClipContent, AppError>;
    /// 貼り付けに使った履歴を先頭に繰り上げる。
    fn promote(&self, id: i64) -> Result<(), AppError>;
}
