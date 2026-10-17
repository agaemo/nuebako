// 骨組み(P1-4)。本体は各トラックで実装し、P3-1 でこの属性を外す。
#![allow(dead_code, unused_variables)]

//! 自己書き込みの抑止(貼り付けで書き込んだ内容を、履歴に記録しない)。track-clipboard が実装する。

#[cfg(test)]
mod tests {
    #[test]
    fn compiled() {}
}
