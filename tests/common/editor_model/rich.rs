use magit_lsp::protocol::{Cursor, LineEdit, ViewRender};
use magit_lsp::view::ViewMode;
use std::collections::BTreeMap;

/// Applied state for one independently rendered view.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ViewState {
    /// Exact server-provided document lines.
    pub lines: Vec<String>,
    /// Last successfully applied render version.
    pub version: u64,
    /// Server-provided cursor position.
    pub cursor: Option<Cursor>,
    /// Display mode for this view.
    pub mode: ViewMode,
    /// Pending menu key sequence.
    pub pending: String,
    /// Structured menu metadata.
    pub menu: Option<serde_json::Value>,
}

/// Client response intent after applying a render.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Intent { Ack(u64), Resync }

/// Per-view fake editor for rich render behavior.
#[derive(Default)]
pub struct RichEditor { /// Render state keyed by server view identifier.
    pub views: BTreeMap<String, ViewState>,
}

impl RichEditor {
    /// Apply render text and metadata, returning acknowledgement or resync intent.
    pub fn apply(&mut self, render: ViewRender) -> Intent {
        let state = self.views.entry(render.view_id).or_default();
        if render.text.is_none() && render.edits.is_some() && render.base_version != Some(state.version) { return Intent::Resync; }
        if let Some(text) = render.text { state.lines = text; }
        else if let Some(mut edits) = render.edits {
            edits.sort_by_key(|edit| std::cmp::Reverse(edit.start_line));
            for LineEdit { start_line, end_line, lines } in edits { state.lines.splice(start_line..end_line, lines); }
        }
        state.version = render.version;
        state.cursor = render.cursor;
        state.mode = render.mode;
        state.pending = render.pending;
        state.menu = render.menu.map(|menu| serde_json::to_value(menu).expect("menu serializes"));
        Intent::Ack(state.version)
    }
}
