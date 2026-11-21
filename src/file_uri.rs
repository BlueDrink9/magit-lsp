//! Conversion between native filesystem paths and `file://` URIs.
//!
//! Both production encoders (tree navigation and the editor draft registry)
//! go through [`uri_from_path`], so native separators, Windows drive, verbatim
//! and UNC prefixes, spaces, Unicode and reserved characters are never
//! assembled by hand. The reverse direction validates the authority, percent
//! escapes and UTF-8 before handing a path to callers.

use lsp_types::Uri;
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// A `file://` URI naming the absolute native `path`.
///
/// [`url::Url::from_file_path`] percent-encodes reserved characters and
/// Unicode and maps Windows drive, verbatim and UNC prefixes to their URI
/// form. Relative paths have no file URI and yield `None`.
pub fn uri_from_path(path: &Path) -> Option<Uri> {
    let url = url::Url::from_file_path(path).ok()?;
    Uri::from_str(url.as_str()).ok()
}

/// The `file://` URI of the repository-relative `relative` path inside the
/// native `repo_root`.
pub fn uri_for_repo_path(repo_root: &Path, relative: &str) -> Option<Uri> {
    uri_from_path(&repo_root.join(relative))
}

/// The native path named by the `file://` `uri`.
///
/// The URI must use the `file` scheme with no authority or `localhost`, the
/// same local ownership the server accepts everywhere; remote authorities are
/// rejected even on Windows, where [`url::Url::to_file_path`] would otherwise
/// decode a UNC server into a path. Malformed percent escapes, percent-encoded
/// bytes that are not UTF-8 and non-file schemes are also rejected, so callers
/// never receive a path decoded from a damaged or non-local URI.
pub fn path_from_uri(uri: &str) -> Option<PathBuf> {
    if !has_valid_percent_escapes(uri) {
        return None;
    }
    let url = url::Url::parse(uri).ok()?;
    if url.scheme() != "file" {
        return None;
    }
    if !matches!(url.host(), None | Some(url::Host::Domain("localhost"))) {
        return None;
    }
    let path = url.to_file_path().ok()?;
    // A rejected non-UTF-8 decode must not reach callers: existing native path
    // handling and user-facing diagnostics assume UTF-8 repository paths.
    path.to_str()?;
    Some(path)
}

/// Whether every `%` in `uri` starts a two-hex-digit escape.
fn has_valid_percent_escapes(uri: &str) -> bool {
    let bytes = uri.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let Some(high) = bytes.get(index + 1).copied() else {
                return false;
            };
            let Some(low) = bytes.get(index + 2).copied() else {
                return false;
            };
            if !high.is_ascii_hexdigit() || !low.is_ascii_hexdigit() {
                return false;
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{LineNode, Link, NodeKind, Tree};
    use lsp_types::Position;

    #[test]
    fn native_absolute_paths_with_specials_round_trip() {
        let root = std::env::temp_dir();
        for relative in ["a b.txt", "é ünïcode/文件.txt", "hash#percent%/a%20b.txt"] {
            let path = root.join(relative);
            let uri = uri_from_path(&path).expect("absolute path has a file URI");
            assert!(uri.as_str().starts_with("file:"), "{}", uri.as_str());
            assert_eq!(
                path_from_uri(uri.as_str()).as_deref(),
                Some(path.as_path()),
                "{}",
                uri.as_str()
            );
        }
        assert_eq!(uri_from_path(Path::new("relative/path")), None);
    }

    #[test]
    fn tree_navigation_targets_use_the_shared_converter() {
        let root = std::env::temp_dir().join("repo root é#%");
        let relative = "dir/a b%#é.txt";
        let tree = Tree {
            version: 1,
            lines: vec![relative.to_owned()],
            nodes: Vec::new(),
            line_nodes: vec![LineNode {
                node: "file".to_owned(),
                kind: NodeKind::File,
                section: None,
                path: Some(relative.to_owned()),
                hunk_id: None,
                status: None,
                marker: None,
                line_index: None,
                new_line: None,
                text: None,
            }],
            tokens: Vec::new(),
            symbols: Vec::new(),
            links: vec![Link {
                line: 0,
                start: 0,
                length: relative.len(),
                path: relative.to_owned(),
            }],
        };
        let expected = uri_for_repo_path(&root, relative).expect("native path has a file URI");
        let root = root.to_string_lossy();

        let links = tree.document_links(&root);
        assert_eq!(
            links[0].target.as_ref().map(|uri| uri.as_str()),
            Some(expected.as_str())
        );
        let location = tree
            .definition(&root, Position::new(0, 0))
            .expect("file line has a definition");
        assert_eq!(location.uri.as_str(), expected.as_str());
    }

    #[test]
    fn malformed_escapes_and_non_file_schemes_are_rejected() {
        for invalid in [
            "https://example.test/a",
            "file://remote/tmp/a",
            "file:///C:/tmp/%Q0",
            "file:///C:/tmp/%FF",
            "file:///C:/tmp/%2",
            "relative/uri",
        ] {
            assert_eq!(path_from_uri(invalid), None, "accepted {invalid}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn unix_file_uris_decode_percent_escapes_and_local_authority() {
        assert_eq!(
            path_from_uri("file:///tmp/a%20%C3%A9%23%25").as_deref(),
            Some(Path::new("/tmp/a é#%"))
        );
        assert_eq!(
            path_from_uri("file://localhost/tmp/a%20b").as_deref(),
            Some(Path::new("/tmp/a b"))
        );
        assert_eq!(path_from_uri("file://remote/tmp/a"), None);
    }

    #[cfg(windows)]
    #[test]
    fn windows_file_uris_decode_drive_verbatim_and_unc_paths() {
        let drive = uri_from_path(Path::new(r"C:\repo\a b\é#%.txt")).expect("drive path");
        assert_eq!(drive.as_str(), "file:///C:/repo/a%20b/%C3%A9%23%25.txt");
        assert_eq!(
            path_from_uri(drive.as_str()).as_deref(),
            Some(Path::new(r"C:\repo\a b\é#%.txt"))
        );

        // Verbatim prefixes are canonicalized to the equivalent local drive.
        let verbatim = uri_from_path(Path::new(r"\\?\C:\repo\a b.txt")).expect("verbatim path");
        assert_eq!(verbatim.as_str(), "file:///C:/repo/a%20b.txt");
        assert_eq!(
            path_from_uri(verbatim.as_str()).as_deref(),
            Some(Path::new(r"C:\repo\a b.txt"))
        );

        let unc = uri_from_path(Path::new(r"\\server\share\a b.txt")).expect("UNC path");
        assert_eq!(unc.as_str(), "file://server/share/a%20b.txt");
        // `from_file_path` can encode UNC, but a remote authority is never
        // decoded back into a locally owned server path.
        assert_eq!(path_from_uri(unc.as_str()), None);
    }
}
