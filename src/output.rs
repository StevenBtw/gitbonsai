//! Writing outputs: through a temp file and a rename, so a failed run keeps
//! the previous files.

use std::path::Path;

use crate::Error;

/// Writes `bytes` to `path`, replacing it only once the new content is
/// complete.
///
/// # Errors
///
/// When the folder can't be created or the file can't be written.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)
            .map_err(|e| Error::io(format!("can't create {}", dir.display()), e))?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    std::fs::write(&tmp, bytes)
        .map_err(|e| Error::io(format!("can't write {}", tmp.display()), e))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        Error::io(format!("can't replace {}", path.display()), e)
    })
}
