//! The server-side handle commands use to ask the user a question (R-3.1).
//!
//! A prompt never reaches Git directly: a handler calls one of [`PromptUi`]'s
//! methods, the transport turns it into a `view/prompt` notification and blocks
//! for the answer, and the handler continues with a plain `Option` where `None`
//! means the user cancelled. Tests use [`ScriptedUi`] to answer without a
//! client and to assert the exact request each handler emitted.
//!
//! The prompt transport is a notification plus a separate `view/promptRespond`
//! request; see TASKS.md §10 for why it deliberately differs from §3.6.

use crate::protocol::{PromptChoiceAnswer, ViewPromptParams};
use crate::{Error, Result};
use std::collections::VecDeque;
use std::sync::Mutex;

/// Every question a command may ask; each returns `None` when cancelled.
pub trait PromptUi {
    /// Ask a yes/no question, returning the answer.
    fn confirm(&self, request: &ViewPromptParams) -> Result<Option<bool>>;

    /// Ask for one line of text.
    fn text(&self, request: &ViewPromptParams) -> Result<Option<String>>;

    /// Pick exactly one item (or a custom value when allowed).
    fn choice(&self, request: &ViewPromptParams) -> Result<Option<PromptChoiceAnswer>>;

    /// Pick zero or more items by id.
    fn multi_choice(&self, request: &ViewPromptParams) -> Result<Option<Vec<String>>>;
}

/// A UI that refuses every prompt; the default for non-interactive callers.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullUi;

impl PromptUi for NullUi {
    fn confirm(&self, _request: &ViewPromptParams) -> Result<Option<bool>> {
        Err(prompts_unavailable())
    }

    fn text(&self, _request: &ViewPromptParams) -> Result<Option<String>> {
        Err(prompts_unavailable())
    }

    fn choice(&self, _request: &ViewPromptParams) -> Result<Option<PromptChoiceAnswer>> {
        Err(prompts_unavailable())
    }

    fn multi_choice(&self, _request: &ViewPromptParams) -> Result<Option<Vec<String>>> {
        Err(prompts_unavailable())
    }
}

fn prompts_unavailable() -> Error {
    Error::new("prompts are unavailable")
}

/// One queued answer; `Cancel` matches any prompt kind.
#[derive(Debug, Clone, PartialEq)]
enum ScriptedAnswer {
    Confirm(Option<bool>),
    Text(Option<String>),
    Choice(Option<PromptChoiceAnswer>),
    Multi(Option<Vec<String>>),
    Cancel,
}

/// A UI answered from a queue, recording every request it received.
#[derive(Debug, Default)]
pub struct ScriptedUi {
    answers: Mutex<VecDeque<ScriptedAnswer>>,
    requests: Mutex<Vec<ViewPromptParams>>,
}

impl ScriptedUi {
    /// An empty UI; script an answer before each prompt.
    pub fn new() -> Self {
        Self::default()
    }

    /// Answer the next confirm prompt with `value`.
    pub fn reply_confirm(&self, value: bool) -> &Self {
        self.push(ScriptedAnswer::Confirm(Some(value)));
        self
    }

    /// Answer the next text prompt with `value`; `None` cancels it.
    pub fn reply_text(&self, value: Option<String>) -> &Self {
        self.push(ScriptedAnswer::Text(value));
        self
    }

    /// Answer the next choice prompt; `None` cancels it.
    pub fn reply_choice(&self, value: Option<PromptChoiceAnswer>) -> &Self {
        self.push(ScriptedAnswer::Choice(value));
        self
    }

    /// Answer the next multi-choice prompt; `None` cancels it.
    pub fn reply_multi(&self, value: Option<Vec<String>>) -> &Self {
        self.push(ScriptedAnswer::Multi(value));
        self
    }

    /// Cancel the next prompt of any kind.
    pub fn reply_cancel(&self) -> &Self {
        self.push(ScriptedAnswer::Cancel);
        self
    }

    /// Every request this UI received, in the order it saw them.
    pub fn requests(&self) -> Vec<ViewPromptParams> {
        self.lock_requests().clone()
    }

    /// The most recent request, if any.
    pub fn last_request(&self) -> Option<ViewPromptParams> {
        self.lock_requests().last().cloned()
    }

    fn push(&self, answer: ScriptedAnswer) {
        let mut answers = self
            .answers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        answers.push_back(answer);
    }

    fn record(&self, request: &ViewPromptParams) {
        self.lock_requests().push(request.clone());
    }

    fn take(&self) -> Result<ScriptedAnswer> {
        self.answers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pop_front()
            .ok_or_else(|| Error::new("ScriptedUi: no scripted answer"))
    }

    fn lock_requests(&self) -> std::sync::MutexGuard<'_, Vec<ViewPromptParams>> {
        self.requests
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl PromptUi for ScriptedUi {
    fn confirm(&self, request: &ViewPromptParams) -> Result<Option<bool>> {
        self.record(request);
        match self.take()? {
            ScriptedAnswer::Confirm(value) => Ok(value),
            ScriptedAnswer::Cancel => Ok(None),
            other => Err(mismatch("confirm", &other)),
        }
    }

    fn text(&self, request: &ViewPromptParams) -> Result<Option<String>> {
        self.record(request);
        match self.take()? {
            ScriptedAnswer::Text(value) => Ok(value),
            ScriptedAnswer::Cancel => Ok(None),
            other => Err(mismatch("text", &other)),
        }
    }

    fn choice(&self, request: &ViewPromptParams) -> Result<Option<PromptChoiceAnswer>> {
        self.record(request);
        match self.take()? {
            ScriptedAnswer::Choice(value) => Ok(value),
            ScriptedAnswer::Cancel => Ok(None),
            other => Err(mismatch("choice", &other)),
        }
    }

    fn multi_choice(&self, request: &ViewPromptParams) -> Result<Option<Vec<String>>> {
        self.record(request);
        match self.take()? {
            ScriptedAnswer::Multi(value) => Ok(value),
            ScriptedAnswer::Cancel => Ok(None),
            other => Err(mismatch("multiChoice", &other)),
        }
    }
}

fn mismatch(kind: &str, answer: &ScriptedAnswer) -> Error {
    Error::new(format!(
        "ScriptedUi: expected a {kind} answer, got {answer:?}"
    ))
}
