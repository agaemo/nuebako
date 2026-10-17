//! 自動ペーストに必要な OS の権限(macOS のアクセシビリティ)。Windows は常に許可。

pub trait PastePermission: Send + Sync {
    fn is_granted(&self) -> bool;
}

// アクセシビリティ権限の判定。引数なしで bool を返すだけの C 関数で、ポインタを受け取らない。
#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

/// 現在の権限の有無。macOS は `AXIsProcessTrusted`、Windows ほかは常に true。
pub fn current_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        // SAFETY: 引数なしで、プロセスが信頼されているかを返すだけの OS API(副作用なし)
        unsafe { AXIsProcessTrusted() }
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// OS に問い合わせる実装。
pub struct SystemPermission;

impl PastePermission for SystemPermission {
    fn is_granted(&self) -> bool {
        current_permission()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn 実機で権限の判定が呼べる() {
        // 値は環境による。パニックせず bool が返ることだけを確認する
        let _ = current_permission();
        let _ = SystemPermission.is_granted();
    }
}
