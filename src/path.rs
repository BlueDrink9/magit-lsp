//! Filesystem path resolution shared by the server.
//!
//! Every production canonicalization goes through these helpers so the
//! resolution rules live in one place and stay consistent across platforms.

use normpath::PathExt;
use std::path::{Path, PathBuf};

/// Resolve `path` to its canonical absolute form.
///
/// On Unix this is equivalent to [`std::fs::canonicalize`]. On Windows it
/// avoids the verbatim `\\?\` prefix that canonicalization would introduce;
/// because it resolves through `GetFullPathNameW` it also does not expand 8.3
/// short-name components (for example a `RUNNER~1` directory). Windows results
/// can therefore retain a short name while [`std::fs::canonicalize`] returns
/// the long one, so never string-compare the two there.
pub fn canonical(path: &Path) -> std::io::Result<PathBuf> {
    Ok(path.normalize()?.into_path_buf())
}

/// Canonicalize `path`, falling back to the input when it cannot be resolved.
///
/// Callers use this where a nonexistent path is legitimate and the raw value
/// must be preserved rather than surfaced as an error.
pub fn canonical_or_raw(path: &Path) -> PathBuf {
    canonical(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn canonical_matches_canonicalize() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            canonical(dir.path()).expect("canonical"),
            std::fs::canonicalize(dir.path()).expect("canonicalize")
        );
    }

    #[cfg(unix)]
    #[test]
    fn canonical_resolves_a_symlinked_parent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("target");
        std::fs::create_dir(&target).expect("create target");
        std::fs::write(target.join("child"), "content").expect("write child");
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");

        assert_eq!(
            canonical(&link.join("child")).expect("canonical through link"),
            std::fs::canonicalize(target.join("child")).expect("canonicalize target child")
        );
    }

    #[test]
    fn canonical_or_raw_returns_a_missing_path_verbatim() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("does-not-exist");
        assert_eq!(canonical_or_raw(&missing), missing);
    }

    #[test]
    fn canonical_or_raw_keeps_a_windows_verbatim_path_verbatim() {
        // An 8.3 short name under a verbatim prefix: canonicalization fails
        // (the path is synthetic) and the fallback must preserve the input
        // exactly, including the short component.
        let synthetic = Path::new(r"\\?\C:\Users\RUNNER~1\tmp\missing");
        assert_eq!(canonical_or_raw(synthetic), synthetic);
    }
}
