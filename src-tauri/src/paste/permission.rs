//! 自動ペーストに必要な OS の権限(macOS のアクセシビリティ)。Windows は常に許可。

pub trait PastePermission: Send + Sync {
    fn is_granted(&self) -> bool;
}

// アクセシビリティ権限の判定。引数なしで C の `Boolean`(UInt8)を返すだけの関数で、ポインタを受け取らない。
// Rust の `bool` で受けると、0 と 1 以外の値が返ったときに未定義動作になるため、`u8` で受ける。
#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> u8;
}

/// C の `Boolean` を `bool` にする。0 以外は真。
#[cfg(any(target_os = "macos", test))]
fn boolean_to_bool(value: u8) -> bool {
    value != 0
}

/// 現在の権限の有無。macOS は `AXIsProcessTrusted`、Windows ほかは常に true。
pub fn current_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        // SAFETY: 引数なしで、プロセスが信頼されているかを返すだけの OS API(副作用なし)
        boolean_to_bool(unsafe { AXIsProcessTrusted() })
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
    fn cのbooleanは0以外を真として扱う() {
        assert!(!boolean_to_bool(0));
        assert!(boolean_to_bool(1));
        // 1 以外の非ゼロでも真にする(Rust の bool で受けると未定義動作になる値)
        assert!(boolean_to_bool(2));
        assert!(boolean_to_bool(255));
    }

    #[test]
    #[ignore]
    fn 実機で権限の判定が呼べる() {
        // 値は環境による。パニックせず bool が返ることだけを確認する
        let _ = current_permission();
        let _ = SystemPermission.is_granted();
    }
}
