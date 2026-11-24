//! Filesystem path resolution shared by the server.
//!
//! Every production canonicalization goes through these helpers so the
//! resolution rules live in one place and stay consistent across platforms.
//!
//! [`canonical`] resolves an existing path to a stable long-form spelling: on
//! Windows the same directory reached through an 8.3 short name (`RUNNER~1`)
//! and its long name (`runneradmin`) yields the same string, so paths from
//! different sources can be compared as strings. On Unix the result is exactly
//! [`std::fs::canonicalize`]. A path that does not exist cannot be resolved
//! that way; it falls back to a lexical normalize, and [`canonical_or_raw`]
//! returns it verbatim. Inputs that were never canonicalized are compared with
//! [`same_object`].

use normpath::PathExt;
use std::path::{Path, PathBuf};

/// Resolve `path` to its canonical absolute form.
///
/// The result is a stable long-form spelling, so two sources that name the
/// same existing filesystem object produce the same string. On Windows that is
/// what [`std::fs::canonicalize`] returns with the verbatim `\\?\` prefix
/// removed from the two normal forms (`\\?\C:\…` becomes `C:\…`,
/// `\\?\UNC\server\share` becomes `\\server\share`; any other verbatim path
/// keeps its prefix), which also expands 8.3 short-name components. On Unix it
/// is exactly [`std::fs::canonicalize`].
///
/// A path that cannot be resolved (it does not exist, or it is otherwise
/// unreachable) falls back to a lexical normalize of the input, which keeps its
/// spelling. Callers that can legitimately see nonexistent paths should use
/// [`canonical_or_raw`] instead.
pub fn canonical(path: &Path) -> std::io::Result<PathBuf> {
    match std::fs::canonicalize(path) {
        Ok(resolved) => Ok(strip_verbatim(resolved)),
        Err(_) => Ok(path.normalize()?.into_path_buf()),
    }
}

/// Remove the verbatim prefix from a canonical Windows path.
///
/// `\\?\C:\…` becomes `C:\…` and `\\?\UNC\server\share` becomes
/// `\\server\share`. Any other verbatim path keeps its prefix.
#[cfg(windows)]
fn strip_verbatim(path: PathBuf) -> PathBuf {
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    const VERBATIM: [u16; 4] = [b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];
    const UNC: [u16; 4] = [b'U' as u16, b'N' as u16, b'C' as u16, b'\\' as u16];

    let wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    if wide.len() >= 7 && wide.starts_with(&VERBATIM) {
        if wide.len() > 8 && wide[4..].starts_with(&UNC) {
            let mut stripped = vec![b'\\' as u16, b'\\' as u16];
            stripped.extend_from_slice(&wide[8..]);
            return PathBuf::from(OsString::from_wide(&stripped));
        }
        if wide[5] == b':' as u16 && wide[6] == b'\\' as u16 {
            return PathBuf::from(OsString::from_wide(&wide[4..]));
        }
    }
    path
}

/// On Unix canonicalization already yields the normal spelling.
#[cfg(not(windows))]
fn strip_verbatim(path: PathBuf) -> PathBuf {
    path
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
/// Use this for inputs that have not been through [`canonical`], such as raw
/// Git output or a path supplied by a client. String comparison is not
/// sufficient there: a path may be spelled with an 8.3 short name
/// (`RUNNER~1`) or its long form (`runneradmin`), or through a symlink, and
/// only the filesystem can establish identity. Paths that have both been
/// canonicalized already share a stable spelling, so comparing them as strings
/// is valid.
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
    fn canonical_resolves_to_a_verbatim_free_stable_spelling() {
        // The permanent guard for path identity across sources: resolving an
        // existing directory drops any verbatim prefix and is idempotent, so
        // short-name and long-name sources converge on the same string.
        let dir = tempfile::tempdir().expect("tempdir");
        let resolved = canonical(dir.path()).expect("canonical");
        assert!(
            !resolved.to_string_lossy().starts_with(r"\\?"),
            "canonical kept a verbatim prefix: {}",
            resolved.display()
        );
        assert_eq!(
            canonical(&resolved).expect("canonical again"),
            resolved,
            "canonical is not idempotent"
        );
    }

    #[test]
    fn same_object_compares_identity_not_spelling() {
        // Two synthetic verbatim paths that cannot be resolved, so the
        // fallback preserves each input's spelling: the short name and the
        // long name stay distinct strings.
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
