//! 保存ファイルの権限。同じ端末の他のユーザーに、暗号化した履歴や設定を読ませないため、
//! unix ではフォルダを 0700、ファイルを 0600 に絞る。Windows はユーザーごとの保存先の ACL に任せる。

use std::path::Path;

/// フォルダを所有者だけが入れるようにする(unix のみ)。
#[cfg_attr(not(unix), allow(unused_variables))]
pub fn restrict_dir(dir: &Path) {
    #[cfg(unix)]
    set_mode(dir, 0o700);
}

/// ファイルを所有者だけが読み書きできるようにする(unix のみ。無ければ何もしない)。
#[cfg_attr(not(unix), allow(unused_variables))]
pub fn restrict_file(path: &Path) {
    #[cfg(unix)]
    if path.exists() {
        set_mode(path, 0o600);
    }
}

/// DB 本体と、SQLite が作る `-wal`・`-shm` のファイル、およびそのフォルダを絞る。
pub fn restrict_db_files(db_path: &Path) {
    restrict_file(db_path);
    for suffix in ["-wal", "-shm"] {
        let mut name = db_path.as_os_str().to_owned();
        name.push(suffix);
        restrict_file(Path::new(&name));
    }
    if let Some(dir) = db_path.parent().filter(|d| !d.as_os_str().is_empty()) {
        restrict_dir(dir);
    }
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    // 失敗しても動作は続ける。パスは出さない
    if std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).is_err() {
        eprintln!("保存ファイルの権限を設定できません");
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn mode(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn dbと_walと_shmは0600でフォルダは0700になる() {
        let dir = crate::storage::repo::test_support::TempDir::new();
        let db = dir.db_path();
        for suffix in ["", "-wal", "-shm"] {
            let mut name = db.as_os_str().to_owned();
            name.push(suffix);
            std::fs::write(&name, b"x").unwrap();
        }
        restrict_db_files(&db);
        assert_eq!(mode(&db), 0o600);
        assert_eq!(mode(&dir.0.join("nuebako.db-wal")), 0o600);
        assert_eq!(mode(&dir.0.join("nuebako.db-shm")), 0o600);
        assert_eq!(mode(&dir.0), 0o700);
    }

    #[test]
    fn 存在しないファイルは作らない() {
        let dir = crate::storage::repo::test_support::TempDir::new();
        restrict_file(&dir.0.join("none"));
        assert!(!dir.0.join("none").exists());
    }
}
