use super::*;

/// Whether the client wants the server to push every view's keymap.
pub(super) fn dynamic_delivery(client: &Arc<Mutex<ClientCaps>>) -> bool {
    let client = client
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    matches!(client.presentation.as_ref(), Some(Presentation::Rich(rich))
        if rich.keys == KeymapDelivery::Dynamic)
}

pub(super) fn custom_ui(client: &Arc<Mutex<ClientCaps>>) -> CustomRender {
    let client = client.clone();
    CustomRender::negotiated(move || {
        match client
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .presentation
            .as_ref()
        {
            Some(Presentation::Rich(rich)) => PromptSupport::Rich(rich.clone()),
            Some(Presentation::Plain(_)) => PromptSupport::Plain,
            None => PromptSupport::Unnegotiated,
        }
    })
}

/// The prompt transport: a `view/prompt` notification and its answer.
///
/// The editor bridge's askpass prompt runs on a thread that outlives the
/// server, so the bridge holds a *weak* sender: it must never keep the writer
/// channel (and with it the server process) alive.
pub(super) struct PromptBridge {
    pub(super) closed: AtomicBool,
    sender: mpsc::WeakUnboundedSender<Message>,
    /// Questions waiting for an answer, keyed by prompt id.
    pending: Mutex<HashMap<String, std_mpsc::Sender<PromptValue>>>,
    /// Prompt-id counter; ids are unique across this server process.
    next_id: AtomicU64,
    client: Arc<Mutex<ClientCaps>>,
}

impl PromptBridge {
    pub(super) fn new(
        sender: &mpsc::UnboundedSender<Message>,
        client: Arc<Mutex<ClientCaps>>,
    ) -> Self {
        Self {
            closed: AtomicBool::new(false),
            sender: sender.downgrade(),
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(0),
            client,
        }
    }

    /// Send one question and block until the client answers it.
    ///
    /// `None` means the user cancelled: `view/promptRespond` drops the sender
    /// without a value, so `recv` fails. A prompt that cannot be delivered (the
    /// writer is gone) is cancelled as well.
    fn ask(&self, request: &ViewPromptParams) -> Option<PromptValue> {
        let ui = custom_ui(&self.client);
        if !ui.allows_prompt(request) {
            if let Some(sender) = self.sender.upgrade() {
                let text = if request.secret == Some(true) {
                    crate::presentation::prompt::SECRET_UNAVAILABLE
                } else {
                    "Client does not support this prompt"
                };
                let _ = sender.send(ui.effects(ViewEffectsParams {
                    effects: vec![Effect::message(MessageLevel::Warn, text)],
                }));
            }
            return None;
        }
        let prompt_id = format!("p{}", self.next_id.fetch_add(1, Ordering::Relaxed) + 1);
        let params = ViewPromptParams {
            prompt_id: prompt_id.clone(),
            ..request.clone()
        };
        let (tx, rx) = std_mpsc::channel();
        {
            let mut pending = self.lock();
            if self.closed.load(Ordering::SeqCst) {
                return None;
            }
            pending.insert(prompt_id.clone(), tx);
        }

        let sent = match self.sender.upgrade() {
            Some(sender) => sender.send(ui.prompt(params)).is_ok(),
            None => false,
        };

        let cancel = runner::current_cancel();
        let answer = if sent {
            loop {
                if cancel.as_ref().is_some_and(|handle| handle.is_cancelled()) {
                    break None;
                }
                match rx.recv_timeout(std::time::Duration::from_millis(50)) {
                    Ok(answer) => break Some(answer),
                    Err(std_mpsc::RecvTimeoutError::Disconnected) => break None,
                    Err(std_mpsc::RecvTimeoutError::Timeout) => {}
                }
            }
        } else {
            None
        };

        self.lock().remove(&prompt_id);
        answer
    }

    /// Deliver an answer to the waiting question.
    pub(super) fn respond(
        &self,
        prompt_id: &str,
        cancelled: bool,
        value: Option<PromptValue>,
    ) -> Result<()> {
        let sender = self
            .lock()
            .remove(prompt_id)
            .ok_or_else(|| Error::new(format!("unknown prompt: {prompt_id}")))?;
        if !cancelled {
            if let Some(value) = value {
                let _ = sender.send(value);
            }
        }
        // Dropping the sender without a value is the cancellation signal.
        Ok(())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, std_mpsc::Sender<PromptValue>>> {
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(super) fn close(&self) {
        let mut pending = self.lock();
        self.closed.store(true, Ordering::SeqCst);
        pending.clear();
    }
}

/// The pick transport: a `view/pick` notification and its answer.
///
/// Like [`PromptBridge`], it holds a *weak* sender so a pending pick never keeps
/// the writer channel (and the server process) alive.
pub(super) struct PickBridge {
    closed: AtomicBool,
    sender: mpsc::WeakUnboundedSender<Message>,
    /// Picks waiting for an answer, keyed by pick id.
    pending: Mutex<HashMap<String, std_mpsc::Sender<ViewPickRespondParams>>>,
    /// Pick-id counter; ids are unique across this server process.
    next_id: AtomicU64,
}

impl PickBridge {
    pub(super) fn new(sender: &mpsc::UnboundedSender<Message>) -> Self {
        Self {
            closed: AtomicBool::new(false),
            sender: sender.downgrade(),
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(0),
        }
    }

    /// Send one pick and block until the client answers it.
    ///
    /// `None` means the user cancelled or the notification could not be
    /// delivered (the writer is gone or the operation was cancelled).
    pub(super) fn ask(
        &self,
        ui: &CustomRender,
        pick: &ViewPickParams,
    ) -> Option<ViewPickRespondParams> {
        let pick_id = format!("k{}", self.next_id.fetch_add(1, Ordering::Relaxed) + 1);
        let params = ViewPickParams {
            pick_id: pick_id.clone(),
            ..pick.clone()
        };
        let (tx, rx) = std_mpsc::channel();
        {
            let mut pending = self.lock();
            if self.closed.load(Ordering::SeqCst) {
                return None;
            }
            pending.insert(pick_id.clone(), tx);
        }

        let sent = match self.sender.upgrade() {
            Some(sender) => sender.send(ui.pick(&params)).is_ok(),
            None => false,
        };

        let cancel = runner::current_cancel();
        let answer = if sent {
            loop {
                if cancel.as_ref().is_some_and(|handle| handle.is_cancelled()) {
                    break None;
                }
                match rx.recv_timeout(std::time::Duration::from_millis(50)) {
                    Ok(answer) => break Some(answer),
                    Err(std_mpsc::RecvTimeoutError::Disconnected) => break None,
                    Err(std_mpsc::RecvTimeoutError::Timeout) => {}
                }
            }
        } else {
            None
        };

        self.lock().remove(&pick_id);
        answer
    }

    /// Deliver an answer to the waiting pick.
    pub(super) fn respond(&self, params: ViewPickRespondParams) -> Result<()> {
        let sender = self
            .lock()
            .remove(&params.pick_id)
            .ok_or_else(|| Error::new(format!("unknown pick: {}", params.pick_id)))?;
        let _ = sender.send(params);
        Ok(())
    }

    fn lock(
        &self,
    ) -> std::sync::MutexGuard<'_, HashMap<String, std_mpsc::Sender<ViewPickRespondParams>>> {
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(super) fn close(&self) {
        let mut pending = self.lock();
        self.closed.store(true, Ordering::SeqCst);
        pending.clear();
    }
}

/// The server's [`PromptUi`]: sends each question to the client and blocks the
/// calling worker thread until the matching `view/promptRespond` arrives.
pub(super) struct ServerUi {
    pub(super) prompts: Arc<PromptBridge>,
}

impl ServerUi {
    pub(super) fn new(shared: &Shared) -> Self {
        Self {
            prompts: shared.prompts.clone(),
        }
    }

    /// Ask one question and convert the wire value to the expected kind.
    fn ask(&self, request: &ViewPromptParams) -> Result<Option<PromptValue>> {
        Ok(self.prompts.ask(request))
    }
}

impl PromptUi for ServerUi {
    fn confirm(&self, request: &ViewPromptParams) -> Result<Option<bool>> {
        match self.ask(request)? {
            Some(PromptValue::Bool(value)) => Ok(Some(value)),
            Some(other) => Err(Error::new(format!("expected a bool answer, got {other:?}"))),
            None => Ok(None),
        }
    }

    fn text(&self, request: &ViewPromptParams) -> Result<Option<String>> {
        match self.ask(request)? {
            Some(PromptValue::Text(value)) => Ok(Some(value)),
            Some(other) => Err(Error::new(format!("expected a text answer, got {other:?}"))),
            None => Ok(None),
        }
    }

    fn choice(&self, request: &ViewPromptParams) -> Result<Option<PromptChoiceAnswer>> {
        match self.ask(request)? {
            Some(PromptValue::Choice(value)) => Ok(Some(value)),
            Some(other) => Err(Error::new(format!(
                "expected a choice answer, got {other:?}"
            ))),
            None => Ok(None),
        }
    }

    fn multi_choice(&self, request: &ViewPromptParams) -> Result<Option<Vec<String>>> {
        match self.ask(request)? {
            Some(PromptValue::Multi { ids }) => Ok(Some(ids)),
            Some(other) => Err(Error::new(format!(
                "expected a multiChoice answer, got {other:?}"
            ))),
            None => Ok(None),
        }
    }
}

/// Recompute and push `view/keymap` for every open view after a config reload.
pub(super) fn refresh_view_keymaps(shared: &Shared, sender: &mpsc::UnboundedSender<Message>) {
    if !shared.dynamic_keymaps() {
        return;
    }
    let sessions: Vec<_> = shared
        .sessions
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .iter()
        .map(|(root, handle)| (root.clone(), handle.clone()))
        .collect();
    for (root, handle) in sessions {
        let mut session = handle.lock().unwrap_or_else(|p| p.into_inner());
        session.set_keymaps(shared.config.for_repository(Path::new(&root)).keymaps());
        let ids: Vec<_> = session
            .views()
            .iter()
            .map(|v| v.descriptor().id.clone())
            .collect();
        for id in ids {
            if let Some(keymap) = session.keymap_notice(&id) {
                let _ = sender.send(Message::Notification(Notification::new(
                    VIEW_KEYMAP.to_string(),
                    keymap,
                )));
            }
        }
    }
}
