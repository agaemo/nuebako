// 骨組み(P1-4)。本体は各トラックで実装し、P3-1 でこの属性を外す。
#![allow(dead_code, unused_variables)]

//! 履歴サービス(track-store が実装する)。

use super::{ClipProvider, ClipRecorder, RecordOutcome};
use crate::domain::clip::{ClipContent, NewClip};
use crate::domain::dto::{ClipDetail, ClipSummary};
use crate::error::AppError;

/// 検索結果。
pub struct SearchResult {
    /// 履歴の全件数(検索条件に関わらない)
    pub total: u32,
    pub items: Vec<ClipSummary>,
}

pub struct HistoryService {
    _private: (),
}

impl HistoryService {
    /// 履歴を検索する。ピン留めを先頭に、残りは新しい順。空のクエリは全件。
    pub fn search(&self, query: &str) -> Result<SearchResult, AppError> {
        unimplemented!("track-store")
    }

    pub fn detail(&self, id: i64) -> Result<ClipDetail, AppError> {
        unimplemented!("track-store")
    }

    pub fn delete(&self, id: i64) -> Result<(), AppError> {
        unimplemented!("track-store")
    }

    /// 全件を削除し、削除した件数を返す。
    pub fn clear(&self) -> Result<u32, AppError> {
        unimplemented!("track-store")
    }

    pub fn set_pinned(&self, id: i64, pinned: bool) -> Result<(), AppError> {
        unimplemented!("track-store")
    }

    /// 上限を適用すると削除される件数(ピン留めは除く)。
    pub fn count_over_limits(&self, max_items: u32, retention_days: u32) -> Result<u32, AppError> {
        unimplemented!("track-store")
    }

    /// 上限を適用して、削除した件数を返す。
    pub fn apply_limits(&self, max_items: u32, retention_days: u32) -> Result<u32, AppError> {
        unimplemented!("track-store")
    }
}

impl ClipRecorder for HistoryService {
    fn record(&self, new: NewClip) -> Result<RecordOutcome, AppError> {
        unimplemented!("track-store")
    }
}

impl ClipProvider for HistoryService {
    fn get_content(&self, id: i64) -> Result<ClipContent, AppError> {
        unimplemented!("track-store")
    }

    fn promote(&self, id: i64) -> Result<(), AppError> {
        unimplemented!("track-store")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn compiled() {}
}
