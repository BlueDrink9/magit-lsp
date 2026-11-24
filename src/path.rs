//! Filesystem path resolution shared by the server.
//!
//! Every production canonicalization goes through these helpers so the
//! resolution rules live in one place and stay consistent across platforms.
//!
//! Canonical form is a spelling, not an identity: on Windows one directory can
//! be reached through both an 8.3 short name and a long name, and these helpers
//! preserve whichever spelling they were given. Compare paths that originate
//! from different sources (Git output, a client-supplied path) with
//! [`same_object`], never as strings.

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

/// Report whether `left` and `right` name the same filesystem object.
///
/// String comparison of our-form paths is not sufficient on Windows: the same
/// directory can be spelled with an 8.3 short name (`RUNNER~1`) or its long
/// form (`runneradmin`), and [`canonical`] deliberately preserves whichever
/// spelling it is given. Use this identity check whenever the two sides come
/// from different sources, such as Git output versus a client-supplied path.
pub fn same_object(left: &Path, right: &Path) -> std::io::Result<bool> {
    Ok(same_file::Handle::from_path(left)? == same_file::Handle::from_path(right)?)
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
    fn same_object_compares_identity_not_spelling() {
        // The same directory under an 8.3 short name and its long name: our
        // canonical form keeps each spelling, so string equality cannot
        // establish identity across sources.
        let short = Path::new(r"\\?\C:\Users\RUNNER~1\tmp\missing");
        let long = Path::new(r"\\?\C:\Users\runneradmin\tmp\missing");
        assert_ne!(canonical_or_raw(short), canonical_or_raw(long));

        #[cfg(unix)]
        {
            let dir = tempfile::tempdir().expect("tempdir");
            let real = dir.path().join("real");
            std::fs::create_dir(&real).expect("create real");
            let alias = dir.path().join("alias");
            std::os::unix::fs::symlink(&real, &alias).expect("symlink");
            let other = dir.path().join("other");
            std::fs::create_dir(&other).expect("create other");

            assert!(same_object(&real, &alias).expect("same directory via alias"));
            assert!(!same_object(&real, &other).expect("distinct directories"));
        }
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
