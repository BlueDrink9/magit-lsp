//! Menus (Magit's transients) and their text rendering (§3.11).
//!
//! A menu is a server-owned view: the server lays it out from the client's
//! viewport, renders text and semantic tokens, and answers every key itself.
//! Clients only display the text and forward keys; the structured [`Menu`] is
//! also attached to `view/render` for clients that prefer native widgets.

use crate::tree::{LineNode, Node, NodeKind, Token, Tree};
use crate::view::{Ctx, Scope, View, ViewKind, Viewport};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// How an option's value is collected. Prompts arrive in R-3.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Reader {
    Text,
    Choice,
    /// keep-for: TASKS §3.11 reader schema (Phase-2/R-2.7); metadata retained, not a specialized executor.
    Path,
    Ref,
    Commit,
    Date,
    Number,
}

/// Whether a menu item can run yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum MenuStatus {
    Implemented,
    /// Listed but not built yet; the roadmap item it will arrive with.
    Planned {
        #[serde(rename = "roadmapId")]
        roadmap_id: String,
    },
}

/// One switch, option or action of a menu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Item {
    /// A boolean flag; its key is `-` plus a letter.
    Switch {
        key: String,
        flag: String,
        label: String,
        on: bool,
        status: MenuStatus,
    },
    /// A flag with a value; its key is `=` plus a letter.
    Option {
        key: String,
        flag: String,
        label: String,
        value: Option<String>,
        reader: Reader,
        status: MenuStatus,
    },
    /// A single-key operation.
    Action {
        key: String,
        command: String,
        label: String,
        status: MenuStatus,
    },
}

impl Item {
    /// The canonical menu key, e.g. `-a`, `=A` or `c`.
    pub fn key(&self) -> &str {
        match self {
            Item::Switch { key, .. } | Item::Option { key, .. } | Item::Action { key, .. } => key,
        }
    }

    /// The visible label.
    pub fn label(&self) -> &str {
        match self {
            Item::Switch { label, .. }
            | Item::Option { label, .. }
            | Item::Action { label, .. } => label,
        }
    }

    /// The item's current status.
    pub fn status(&self) -> &MenuStatus {
        match self {
            Item::Switch { status, .. }
            | Item::Option { status, .. }
            | Item::Action { status, .. } => status,
        }
    }

    /// Whether the item is an action.
    pub fn is_action(&self) -> bool {
        matches!(self, Item::Action { .. })
    }

    /// Whether the item is a switch.
    pub fn is_switch(&self) -> bool {
        matches!(self, Item::Switch { .. })
    }

    /// Whether the item is an option.
    pub fn is_option(&self) -> bool {
        matches!(self, Item::Option { .. })
    }
}

/// A titled group of items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    pub title: String,
    #[serde(serialize_with = "serialize_items")]
    pub items: Vec<Item>,
}

fn serialize_items<S: serde::Serializer>(items: &[Item], serializer: S) -> Result<S::Ok, S::Error> {
    #[derive(Serialize)]
    struct SerializedItem<'a> {
        #[serde(flatten)]
        item: &'a Item,
        #[serde(rename = "activationSteps")]
        activation_steps: Vec<String>,
    }
    items
        .iter()
        .map(|item| SerializedItem {
            item,
            activation_steps: menu_key_steps(item.key()),
        })
        .collect::<Vec<_>>()
        .serialize(serializer)
}

/// Interpret one validated canonical menu key as the steps clients must send.
fn menu_key_steps(key: &str) -> Vec<String> {
    if key.starts_with('-') || key.starts_with('=') {
        let (_, suffix) = key.split_at(1);
        if suffix.chars().count() == 1 {
            return vec![key[..1].to_string(), suffix.to_string()];
        }
    }
    key.split(' ').map(str::to_string).collect()
}

/// A saved set of switches and option values.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Args {
    /// Keys of the switches that are on, e.g. `-a`.
    pub switches: Vec<String>,
    /// Set options, keyed by menu key, e.g. `=A` -> `"Alice"`.
    pub options: BTreeMap<String, String>,
}

/// A menu: its definition plus the current switch/option state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Menu {
    pub id: String,
    pub title: String,
    pub groups: Vec<Group>,
    /// The arguments `menu.reset-args` restores.
    pub saved_args: Args,
}

impl Menu {
    /// The command the argument preview starts with.
    fn base_command(&self) -> String {
        crate::menus::base_command(&self.id)
            .map(str::to_string)
            .unwrap_or_else(|| format!("magit-lsp {}", self.id))
    }

    /// The switches and options currently set.
    pub fn args(&self) -> Args {
        let mut switches = Vec::new();
        let mut options = BTreeMap::new();
        for group in &self.groups {
            for item in &group.items {
                match item {
                    Item::Switch { key, on: true, .. } => switches.push(key.clone()),
                    Item::Option {
                        key,
                        value: Some(value),
                        ..
                    } => {
                        options.insert(key.clone(), value.clone());
                    }
                    _ => {}
                }
            }
        }
        Args { switches, options }
    }

    /// Apply `args`, turning exactly those switches on and options set.
    pub fn apply_args(&mut self, args: &Args) {
        for group in &mut self.groups {
            for item in &mut group.items {
                match item {
                    Item::Switch { key, on, .. } => *on = args.switches.contains(key),
                    Item::Option { key, value, .. } => {
                        *value = args.options.get(key).cloned();
                    }
                    Item::Action { .. } => {}
                }
            }
        }
    }

    /// Remember the current arguments as the reset defaults.
    pub fn save_args(&mut self) {
        self.saved_args = self.args();
    }

    /// Restore the saved arguments.
    pub fn reset_args(&mut self) {
        let saved = self.saved_args.clone();
        self.apply_args(&saved);
    }

    /// Flip the switch with key `key`; returns `false` when there is none.
    pub fn toggle_switch(&mut self, key: &str) -> bool {
        for group in &mut self.groups {
            for item in &mut group.items {
                if let Item::Switch {
                    key: item_key, on, ..
                } = item
                {
                    if item_key == key {
                        *on = !*on;
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Set an option's value; `None` clears it.
    pub fn set_option(&mut self, key: &str, value: Option<String>) -> bool {
        for group in &mut self.groups {
            for item in &mut group.items {
                if let Item::Option {
                    key: item_key,
                    value: current,
                    ..
                } = item
                {
                    if item_key == key {
                        *current = value;
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Find an item by its canonical key.
    pub fn item(&self, key: &str) -> Option<&Item> {
        self.groups
            .iter()
            .flat_map(|group| &group.items)
            .find(|item| item.key() == key)
    }

    /// Find a mutable item by its canonical key.
    pub fn item_mut(&mut self, key: &str) -> Option<&mut Item> {
        self.groups
            .iter_mut()
            .flat_map(|group| &mut group.items)
            .find(|item| item.key() == key)
    }

    /// The argument preview line, e.g. `git commit --all --no-verify`.
    pub fn preview(&self) -> String {
        let mut parts = vec![self.base_command()];
        for group in &self.groups {
            for item in &group.items {
                match item {
                    Item::Switch { flag, on: true, .. } => parts.push(flag.clone()),
                    Item::Option {
                        flag,
                        value: Some(value),
                        ..
                    } => parts.push(format!("{flag}{value}")),
                    _ => {}
                }
            }
        }
        parts.join(" ")
    }

    /// Render the menu as a tree laid out for `viewport`.
    pub fn render(&self, viewport: Viewport) -> Tree {
        self.render_with_pending(viewport, &[])
    }

    /// Render with the menu's pending key steps, dimming items that cannot
    /// complete them and highlighting the prefix of the ones that can.
    pub fn render_with_pending(&self, viewport: Viewport, pending: &[String]) -> Tree {
        let mut builder = Builder::new(self, pending);
        builder.title();
        builder.arguments(viewport.cols as usize);
        builder.preview();
        builder.actions(viewport.cols as usize);
        builder.finish()
    }
}

/// Whether `key` can still complete after the pending menu steps.
pub fn key_matches_pending(key: &str, pending: &[String]) -> bool {
    if pending.is_empty() {
        return true;
    }
    if pending.len() == 1 && matches!(pending[0].as_str(), "-" | "=") {
        return key.starts_with(&pending[0]);
    }
    let prefix = pending.join(" ");
    key == prefix || key.starts_with(&format!("{prefix} "))
}

/// A menu view: a [`Menu`] rendered through the ordinary view pipeline.
pub struct MenuView {
    menu: Menu,
}

impl MenuView {
    /// Wrap `menu` in a view.
    pub fn new(menu: Menu) -> Self {
        Self { menu }
    }
}

impl View for MenuView {
    fn kind(&self) -> ViewKind {
        ViewKind::Menu(format!("menu.{}", self.menu.id))
    }

    fn title(&self) -> String {
        self.menu.title.clone()
    }

    fn scope(&self) -> Scope {
        Scope::new(format!("menu.{}", self.menu.id))
    }

    fn capture(&self) -> bool {
        true
    }

    fn render(&mut self, ctx: &Ctx<'_>) -> Tree {
        self.menu.render_with_pending(ctx.viewport, ctx.pending)
    }

    fn menu(&self) -> Option<Menu> {
        Some(self.menu.clone())
    }

    fn menu_mut(&mut self) -> Option<&mut Menu> {
        Some(&mut self.menu)
    }
}

/// The text and structure builder. Keeping layout in one place makes the
/// column reflow on `view/resize` a pure function of the viewport.
struct Builder<'a> {
    menu: &'a Menu,
    pending: &'a [String],
    tree: Tree,
}

impl<'a> Builder<'a> {
    fn new(menu: &'a Menu, pending: &'a [String]) -> Self {
        Self {
            menu,
            pending,
            tree: Tree {
                version: 0,
                lines: Vec::new(),
                nodes: Vec::new(),
                line_nodes: Vec::new(),
                tokens: Vec::new(),
                symbols: Vec::new(),
                links: Vec::new(),
            },
        }
    }

    fn title(&mut self) {
        let line = self.tree.lines.len();
        let id = format!("menu:{}/title", self.menu.id);
        self.tree.lines.push(format!(" {}", self.menu.title));
        self.tree.line_nodes.push(line_node(&id, NodeKind::Header));
        self.tree
            .nodes
            .push(node(&id, NodeKind::Header, self.menu.title.clone(), line));
        self.token(line, 1, self.menu.title.len(), "magit.menu.title");
    }

    /// The `Arguments` group is vertical: one switch or option per line,
    /// wrapped so every line fits `cols`.
    fn arguments(&mut self, cols: usize) {
        let arguments: Vec<&Group> = self
            .menu
            .groups
            .iter()
            .filter(|group| group.title == "Arguments")
            .collect();
        for group in arguments {
            let group_id = format!("menu:{}/group:{}", self.menu.id, slug(&group.title));
            let start = self.tree.lines.len();
            let line = self.tree.lines.len();
            self.tree.lines.push(format!(" {}", group.title));
            self.tree
                .line_nodes
                .push(line_node(&group_id, NodeKind::Section));
            self.token(line, 1, group.title.len(), "magit.menu.group");

            let index = self.tree.nodes.len();
            let mut group_node = node(&group_id, NodeKind::Section, group.title.clone(), start);
            self.tree.nodes.push(group_node.clone());

            let mut children = Vec::new();
            for item in &group.items {
                children.push(self.argument_item(item, cols));
            }
            group_node.children = children;
            group_node.end_line = self.tree.lines.len().saturating_sub(1);
            self.tree.nodes[index] = group_node;
        }
    }

    fn argument_item(&mut self, item: &Item, cols: usize) -> String {
        let key = item.key();
        let label = item.label();
        let id = format!("menu:{}/item:{key}", self.menu.id);
        let mut text = format!("  {key}  {label}");
        let live = self.item_live(key);
        let label_start = 2 + key.len() + 2;
        let mut tokens = self.item_tokens(0, item);

        if let Item::Switch { flag, on, .. } = item {
            text.push_str(&format!(" ({flag})"));
            let flag_start = label_start + label.len() + 2;
            let flag_type = if !live {
                "magit.menu.disabled"
            } else if *on {
                "magit.menu.flag.on"
            } else {
                "magit.menu.flag.off"
            };
            tokens.push((flag_start, flag.len(), flag_type));
            text.push_str("  ");
            let marker = if *on { "[x]" } else { "[ ]" };
            let marker_start = text.len();
            text.push_str(marker);
            let marker_type = if !live {
                "magit.menu.disabled"
            } else if *on {
                "magit.menu.switch.on"
            } else {
                "magit.menu.switch.off"
            };
            tokens.push((marker_start, 3, marker_type));
        } else if let Item::Option { flag, value, .. } = item {
            text.push_str(&format!(" ({flag})"));
            let flag_start = label_start + label.len() + 2;
            tokens.push((
                flag_start,
                flag.len(),
                if live {
                    "magit.menu.flag"
                } else {
                    "magit.menu.disabled"
                },
            ));
            if let Some(value) = value {
                text.push_str("  ");
                let value_start = text.len();
                text.push_str(value);
                tokens.push((
                    value_start,
                    value.len(),
                    if live {
                        "magit.menu.value"
                    } else {
                        "magit.menu.disabled"
                    },
                ));
            }
        }

        // Wrap the line so it fits the viewport; a continuation keeps the
        // key/label/flag/marker tokens over exactly their text.
        let wrapped = wrap_line(&text, &tokens, cols, 2 + key.len() + 2);
        let total = wrapped.len();
        let first_line = self.tree.lines.len();
        for (offset, (line_text, line_tokens)) in wrapped.into_iter().enumerate() {
            let line = first_line + offset;
            self.tree.lines.push(line_text);
            for (start, length, token_type) in line_tokens {
                self.token(line, start, length, token_type);
            }
            let mut entry = line_node(&id, NodeKind::Line);
            entry.status = Some(status_name(item.status()));
            self.tree.line_nodes.push(entry);
            if offset == 0 {
                let mut node = item_node(&id, item, line);
                node.end_line = first_line + total - 1;
                self.tree.nodes.push(node);
            }
        }
        id
    }

    /// Whether an item can still complete the pending steps.
    fn item_live(&self, key: &str) -> bool {
        key_matches_pending(key, self.pending)
    }

    /// Key and label tokens for an item whose key starts at `base + 2`.
    fn item_tokens(&self, base: usize, item: &Item) -> Vec<(usize, usize, &'static str)> {
        let key = item.key();
        let label = item.label();
        let key_start = base + 2;
        let label_start = key_start + key.len() + 2;
        let mut tokens = Vec::new();
        if !self.item_live(key) {
            tokens.push((key_start, key.len(), "magit.menu.disabled"));
            tokens.push((label_start, label.len(), "magit.menu.disabled"));
            return tokens;
        }
        let pending = self.pending.join(" ");
        if !pending.is_empty() && key.starts_with(&pending) {
            tokens.push((key_start, pending.len(), "magit.menu.pending"));
            tokens.push((
                key_start + pending.len(),
                key.len() - pending.len(),
                key_token(item),
            ));
        } else {
            tokens.push((key_start, key.len(), key_token(item)));
        }
        tokens.push((label_start, label.len(), label_token(item)));
        tokens
    }

    /// The preview line follows the arguments it describes.
    fn preview(&mut self) {
        if !self
            .menu
            .groups
            .iter()
            .any(|group| group.title == "Arguments")
        {
            return;
        }
        let line = self.tree.lines.len();
        let text = self.menu.preview();
        let id = format!("menu:{}/preview", self.menu.id);
        self.tree.lines.push(format!(" {text}"));
        self.tree.line_nodes.push(line_node(&id, NodeKind::Message));
        self.tree
            .nodes
            .push(node(&id, NodeKind::Message, text.clone(), line));
        self.token(line, 1, text.len(), "magit.menu.preview");
    }

    /// Action groups are laid out side by side, wrapping by viewport width.
    fn actions(&mut self, cols: usize) {
        let groups: Vec<Group> = self
            .menu
            .groups
            .iter()
            .filter(|group| group.title != "Arguments")
            .cloned()
            .collect();
        if groups.is_empty() {
            return;
        }

        let blocks: Vec<Block> = groups.iter().map(Block::new).collect();
        let rows = wrap(&blocks, cols.max(1));

        for row in rows {
            let start = self.tree.lines.len();
            let height = row.iter().map(|block| block.height()).max().unwrap_or(0);

            // Register each group node first; its line node comes from the
            // column row's first line.
            for block in &row {
                let group_id = format!("menu:{}/group:{}", self.menu.id, slug(&block.group.title));
                let mut group_node = node(
                    &group_id,
                    NodeKind::Section,
                    block.group.title.clone(),
                    start,
                );
                group_node.end_line = start + block.height().saturating_sub(1);
                group_node.children = block
                    .group
                    .items
                    .iter()
                    .map(|item| format!("menu:{}/item:{}", self.menu.id, item.key()))
                    .collect();
                self.tree.nodes.push(group_node);
            }

            for line_offset in 0..height {
                let line = start + line_offset;
                let mut text = String::new();
                let mut tokens: Vec<(usize, usize, &'static str)> = Vec::new();
                let mut owner: Option<(&str, &Item)> = None;
                for block in &row {
                    let content = block.content(line_offset);
                    let width = block.width;
                    let base = text.len();
                    if let Some(content) = content {
                        if line_offset == 0 {
                            self.token(line, base + 1, block.group.title.len(), "magit.menu.group");
                        } else {
                            let item = &block.group.items[line_offset - 1];
                            tokens.extend(self.item_tokens(base, item));
                            owner = Some((item.key(), item));
                        }
                        text.push_str(&content);
                    }
                    if text.len() < base + width {
                        text.push_str(&" ".repeat(base + width - text.len()));
                    }
                }
                let text = text.trim_end().to_string();
                self.tree.lines.push(text);
                for (start, length, token_type) in tokens {
                    self.token(line, start, length, token_type);
                }
                match owner {
                    Some((key, item)) => {
                        let id = format!("menu:{}/item:{key}", self.menu.id);
                        let mut entry = line_node(&id, NodeKind::Line);
                        entry.status = Some(status_name(item.status()));
                        self.tree.line_nodes.push(entry);
                        self.tree.nodes.push(item_node(&id, item, line));
                    }
                    None => {
                        let group = &row[0];
                        let group_id =
                            format!("menu:{}/group:{}", self.menu.id, slug(&group.group.title));
                        self.tree
                            .line_nodes
                            .push(line_node(&group_id, NodeKind::Section));
                    }
                }
            }
        }
    }

    fn token(&mut self, line: usize, start: usize, length: usize, token_type: &str) {
        if length == 0 {
            return;
        }
        self.tree.tokens.push(Token {
            line,
            start,
            length,
            token_type: token_type.to_string(),
            modifiers: Vec::new(),
        });
    }

    fn finish(self) -> Tree {
        self.tree
    }
}

/// One action group as a column block.
struct Block {
    group: Group,
    width: usize,
}

impl Block {
    fn new(group: &Group) -> Self {
        let widest = group
            .items
            .iter()
            .map(|item| format!("  {}  {}", item.key(), item.label()).len())
            .max()
            .unwrap_or(0);
        let width = widest.max(group.title.len() + 1) + 2;
        Self {
            group: group.clone(),
            width,
        }
    }

    fn height(&self) -> usize {
        1 + self.group.items.len()
    }

    fn content(&self, offset: usize) -> Option<String> {
        if offset == 0 {
            Some(format!(" {}", self.group.title))
        } else {
            self.group
                .items
                .get(offset - 1)
                .map(|item| format!("  {}  {}", item.key(), item.label()))
        }
    }
}

/// Greedily place blocks left to right, wrapping at `cols`.
fn wrap<'a>(blocks: &'a [Block], cols: usize) -> Vec<Vec<&'a Block>> {
    let mut rows: Vec<Vec<&Block>> = Vec::new();
    let mut current: Vec<&Block> = Vec::new();
    let mut used = 0;
    for block in blocks {
        if !current.is_empty() && used + block.width > cols {
            rows.push(std::mem::take(&mut current));
            used = 0;
        }
        current.push(block);
        used += block.width;
    }
    if !current.is_empty() {
        rows.push(current);
    }
    rows
}

/// Wrap one rendered argument line so it fits `cols`, breaking at runs of
/// spaces and indenting continuation lines by `indent` spaces so flags and
/// values stay aligned under the label. Nothing is dropped: continuation
/// lines carry the same key, label, flag, value and marker text, and tokens
/// keep covering exactly their text, so a token that spans a wrap point is
/// split in two. A single word wider than the viewport is broken at the
/// column limit.
fn wrap_line(
    text: &str,
    tokens: &[(usize, usize, &'static str)],
    cols: usize,
    indent: usize,
) -> Vec<(String, Vec<(usize, usize, &'static str)>)> {
    if text.len() <= cols || cols == 0 {
        return vec![(text.to_string(), tokens.to_vec())];
    }
    let bytes = text.as_bytes();

    // The non-space runs of `text`; the spaces between them are wrap points.
    let mut words: Vec<(usize, usize)> = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b' ' {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && bytes[index] != b' ' {
            index += 1;
        }
        words.push((start, index));
    }
    // A word wider than the continuation capacity cannot fit any line, so
    // break it at the column limit up front.
    let content_capacity = (cols.saturating_sub(indent)).max(1);
    words = words
        .into_iter()
        .flat_map(|(start, end)| {
            (start..end)
                .step_by(content_capacity)
                .map(move |begin| (begin, (begin + content_capacity).min(end)))
                .collect::<Vec<_>>()
        })
        .collect();

    // Greedy fill. A row is a byte range of `text`, so interior spacing is
    // kept verbatim; the space run at a wrap point is the only thing dropped.
    let mut rows: Vec<(usize, usize)> = Vec::new();
    let mut row_start = 0;
    let mut row_end = 0;
    let mut empty = true;
    for &(start, end) in &words {
        let first_row = rows.is_empty();
        let base = if first_row { 0 } else { indent };
        let capacity = if first_row { cols } else { content_capacity };
        let span_start = if first_row {
            0
        } else if empty {
            start
        } else {
            row_start
        };
        if !empty && base + end - span_start > capacity {
            rows.push((row_start, row_end));
            row_start = start;
        } else {
            row_start = span_start;
        }
        row_end = end;
        empty = false;
    }
    if !empty {
        rows.push((row_start, row_end));
    }

    rows.into_iter()
        .map(|(start, end)| {
            let padding = " ".repeat(if start == 0 { 0 } else { indent });
            let line = format!("{padding}{}", &text[start..end]);
            let offset = padding.len();
            let line_tokens = tokens
                .iter()
                .filter_map(|&(token_start, token_length, token_type)| {
                    let token_end = token_start + token_length;
                    let clipped_start = token_start.max(start);
                    let clipped_end = token_end.min(end);
                    (clipped_start < clipped_end).then(|| {
                        (
                            offset + (clipped_start - start),
                            clipped_end - clipped_start,
                            token_type,
                        )
                    })
                })
                .collect();
            (line, line_tokens)
        })
        .collect()
}

fn key_token(item: &Item) -> &'static str {
    if item.is_option() {
        "magit.menu.option"
    } else {
        "magit.menu.key"
    }
}

fn label_token(item: &Item) -> &'static str {
    match item.status() {
        MenuStatus::Implemented => "magit.menu.label",
        MenuStatus::Planned { .. } => "magit.menu.planned",
    }
}

fn status_name(status: &MenuStatus) -> String {
    match status {
        MenuStatus::Implemented => "implemented".to_string(),
        MenuStatus::Planned { roadmap_id } => format!("planned:{roadmap_id}"),
    }
}

fn slug(title: &str) -> String {
    title
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

fn line_node(id: &str, kind: NodeKind) -> LineNode {
    LineNode {
        node: id.to_string(),
        kind,
        section: None,
        path: None,
        hunk_id: None,
        status: None,
        marker: None,
        line_index: None,
        new_line: None,
        text: None,
    }
}

fn node(id: &str, kind: NodeKind, label: impl Into<String>, start_line: usize) -> Node {
    Node {
        id: id.to_string(),
        kind,
        label: label.into(),
        start_line,
        end_line: start_line,
        section: None,
        path: None,
        hunk_id: None,
        status: None,
        marker: None,
        line_index: None,
        new_line: None,
        copy: None,
        mutation: None,
        commit: None,
        children: Vec::new(),
    }
}

fn item_node(id: &str, item: &Item, line: usize) -> Node {
    let mut node = node(id, NodeKind::Line, item.label().to_string(), line);
    node.status = Some(status_name(item.status()));
    node.copy = Some(item.label().to_string());
    node
}
