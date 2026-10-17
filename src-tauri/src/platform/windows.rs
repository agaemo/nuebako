// 骨組み(P1-4)。本体は各トラックで実装し、P3-1 でこの属性を外す。
#![allow(dead_code, unused_variables)]

//! Windows 固有の実装(`::windows` crate)。track-clipboard が実装する。
//! このファイルでは、モジュール名との衝突を避けるため、crate は `::windows::...` で参照する。

#[cfg(test)]
mod tests {
    #[test]
    fn compiled() {}
}
