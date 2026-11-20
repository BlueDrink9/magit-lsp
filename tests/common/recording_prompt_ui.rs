//! Typed, always-cancel prompt recorder for session tests.

use magit_lsp::protocol::{PromptChoiceAnswer, ViewPromptParams};
use magit_lsp::ui::PromptUi;
use magit_lsp::Result;
use std::cell::RefCell;

/// Records typed prompt requests in order and deterministically cancels them.
#[derive(Default)]
pub struct RecordingPromptUi {
    /// Requests observed by the UI, in delivery order.
    pub requests: RefCell<Vec<ViewPromptParams>>,
}

impl PromptUi for RecordingPromptUi {
    fn confirm(&self, request: &ViewPromptParams) -> Result<Option<bool>> {
        self.requests.borrow_mut().push(request.clone());
        Ok(None)
    }

    fn text(&self, request: &ViewPromptParams) -> Result<Option<String>> {
        self.requests.borrow_mut().push(request.clone());
        Ok(None)
    }

    fn choice(&self, request: &ViewPromptParams) -> Result<Option<PromptChoiceAnswer>> {
        self.requests.borrow_mut().push(request.clone());
        Ok(None)
    }

    fn multi_choice(&self, request: &ViewPromptParams) -> Result<Option<Vec<String>>> {
        self.requests.borrow_mut().push(request.clone());
        Ok(None)
    }
}
