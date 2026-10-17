// 骨組み(P1-4)。本体は各トラックで実装し、P3-1 でこの属性を外す。
#![allow(dead_code, unused_variables)]

//! 自動ペーストに必要な OS の権限(macOS のアクセシビリティ)。Windows は常に許可。

pub trait PastePermission: Send + Sync {
    fn is_granted(&self) -> bool;
}

/// 現在の権限の有無。実装は track-clipboard(macOS は `AXIsProcessTrusted`)。
pub fn current_permission() -> bool {
    true
}

#[cfg(test)]
mod tests {
    #[test]
    fn compiled() {}
}
