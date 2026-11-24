//! Files BOB writes for itself. Settings, memory, usage and the profile can contain personal data, so they
//! are created readable by this user only (0600), written in full to a temporary file and synced before the
//! rename, so a crash or full disk never leaves a half-written file in place.

use std::io::Write;
use std::path::Path;

/// Write `bytes` to `tmp` (created 0600, truncated) and flush it to disk. The caller renames it.
pub fn write_private_file(tmp: &Path, bytes: impl AsRef<[u8]>) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(tmp)?;
    // An existing file keeps its old mode with `open`; make sure it is private.
    use std::os::unix::fs::PermissionsExt;
    f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    f.write_all(bytes.as_ref())?;
    f.sync_all()
}

/// Make a directory BOB owns private to this user (0700), creating it when missing.
pub fn private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn files_are_private_even_when_they_existed_before() {
        let dir = std::env::temp_dir().join(format!("bob-fs-{}", rand::random::<u32>()));
        private_dir(&dir).unwrap();
        let p = dir.join("x.json.tmp");
        std::fs::write(&p, "old").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
        write_private_file(&p, "new").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "new");
        assert_eq!(
            std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
