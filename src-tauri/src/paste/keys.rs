// 骨組み(P1-4)。本体は各トラックで実装し、P3-1 でこの属性を外す。
#![allow(dead_code, unused_variables)]

//! 貼り付けのキー送信(enigo を trait で包む)。macOS は Cmd+V、Windows は Ctrl+V。

use crate::error::AppError;

pub trait KeySender: Send + Sync {
    fn send_paste(&self) -> Result<(), AppError>;
}

#[cfg(test)]
mod tests {
    #[test]
    fn compiled() {}
}
