//! Open text documents synchronized from LSP notifications.

use std::collections::HashMap;

/// LSP negotiated character unit used for line/column positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    /// Columns count UTF-8 bytes.
    Utf8,
    /// Columns count UTF-16 code units (the LSP default).
    Utf16,
    /// Columns count Unicode scalar values.
    Utf32,
}

/// A sequential text change; absent endpoints replace the entire document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// Inclusive line/column start, or absent for full replacement.
    pub start: Option<(u32, u32)>,
    /// Exclusive line/column end, or absent for full replacement.
    pub end: Option<(u32, u32)>,
    /// Replacement text, retained verbatim.
    pub text: String,
}

/// A tracked open document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// Last accepted text, never a partially applied notification.
    pub text: String,
    /// Version of the last accepted snapshot.
    pub version: i32,
    /// Set after a rejected change so callers can request authoritative repair.
    pub repair: bool,
    observed_version: i32,
}

/// URI-keyed store of open documents. Invalid notifications never partially apply.
#[derive(Debug, Default, Clone)]
pub struct DocStore {
    documents: HashMap<String, Document>,
}

impl DocStore {
    /// Record an opened document, replacing any prior entry for its URI.
    pub fn open(&mut self, uri: impl Into<String>, text: String, version: i32) {
        self.documents.insert(
            uri.into(),
            Document {
                text,
                version,
                repair: false,
                observed_version: version,
            },
        );
    }
    /// Remove a document on didClose.
    pub fn close(&mut self, uri: &str) -> Option<Document> {
        self.documents.remove(uri)
    }
    /// Return whether the URI currently represents an open document.
    pub fn is_open(&self, uri: &str) -> bool {
        self.documents.contains_key(uri)
    }
    /// Look up the current document text and version.
    pub fn get(&self, uri: &str) -> Option<&Document> {
        self.documents.get(uri)
    }
    /// Validate versionless save text; it cannot replace the live snapshot or repair it.
    pub fn save_text(&self, uri: &str, text: &str) -> Result<(), ()> {
        let document = self.documents.get(uri).ok_or(())?;
        if document.text == text {
            Ok(())
        } else {
            Err(())
        }
    }
    /// Flag an unknown client state without replacing the last accepted snapshot.
    pub fn require_repair(&mut self, uri: &str, version: Option<i32>) {
        if let Some(document) = self.documents.get_mut(uri) {
            document.repair = true;
            if let Some(version) = version {
                document.observed_version = document.observed_version.max(version);
            }
        }
    }
    /// Apply changes atomically; rejection retains text/version and requires repair.
    pub fn change(
        &mut self,
        uri: &str,
        version: i32,
        changes: &[Change],
        encoding: Encoding,
    ) -> Result<(), ()> {
        let current = self.documents.get(uri).ok_or(())?;
        if version <= current.observed_version {
            self.require_repair(uri, Some(version));
            return Err(());
        }
        let mut text = current.text.clone();
        if current.repair
            && changes
                .first()
                .is_none_or(|c| c.start.is_some() || c.end.is_some())
        {
            self.require_repair(uri, Some(version));
            return Err(());
        }
        let result = (|| {
            for change in changes {
                match (change.start, change.end) {
                    (None, None) => text.clone_from(&change.text),
                    (Some(start), Some(end)) => {
                        let a = offset(&text, start, encoding).ok_or(())?;
                        let b = offset(&text, end, encoding).ok_or(())?;
                        if a > b {
                            return Err(());
                        }
                        text.replace_range(a..b, &change.text);
                    }
                    _ => return Err(()),
                }
            }
            Ok::<(), ()>(())
        })();
        if result.is_err() {
            self.require_repair(uri, Some(version));
            return result;
        }
        let doc = self.documents.get_mut(uri).ok_or(())?;
        doc.text = text;
        doc.version = version;
        doc.observed_version = version;
        doc.repair = false;
        Ok(())
    }
}

fn offset(text: &str, pos: (u32, u32), encoding: Encoding) -> Option<usize> {
    let mut line_start = 0;
    for _ in 0..pos.0 {
        let tail = text.get(line_start..)?;
        let rel = tail.find(['\r', '\n'])?;
        line_start += rel + 1;
        if tail[rel..].starts_with("\r\n") {
            line_start += 1;
        }
    }
    let tail = text.get(line_start..)?;
    let line = &tail[..tail.find(['\r', '\n']).unwrap_or(tail.len())];
    let mut units = 0u32;
    for (i, ch) in line.char_indices() {
        if units == pos.1 {
            return Some(line_start + i);
        }
        units = units.checked_add(match encoding {
            Encoding::Utf8 => ch.len_utf8() as u32,
            Encoding::Utf16 => ch.len_utf16() as u32,
            Encoding::Utf32 => 1,
        })?;
        if units > pos.1 {
            return None;
        }
    }
    (units == pos.1).then_some(line_start + line.len())
}
