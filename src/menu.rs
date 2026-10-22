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
    /// The backend cannot do this; the reason is shown instead.
    Unavailable { reason: String },
}

impl MenuStatus {
    fn implemented() -> Self {
        Self::Implemented
    }

    fn planned(roadmap_id: &str) -> Self {
        Self::Planned {
            roadmap_id: roadmap_id.to_string(),
        }
    }
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
            Item::Switch { label, .. } | Item::Option { label, .. } | Item::Action { label, .. } => {
                label
            }
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
    /// Preferred column count, when a menu overrides the automatic layout.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub columns: Option<u32>,
    pub items: Vec<Item>,
}

impl Group {
    fn new(title: &str, items: Vec<Item>) -> Self {
        Self {
            title: title.to_string(),
            columns: None,
            items,
        }
    }
}

/// A saved set of switches and option values.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    /// The representative commit menu of R-2.7.
    pub fn commit() -> Self {
        Self {
            id: "commit".to_string(),
            title: "Commit".to_string(),
            saved_args: Args::default(),
            groups: vec![
                Group::new(
                    "Arguments",
                    vec![
                        Item::Switch {
                            key: "-a".to_string(),
                            flag: "--all".to_string(),
                            label: "Stage all modified and deleted files".to_string(),
                            on: false,
                            status: MenuStatus::implemented(),
                        },
                        Item::Switch {
                            key: "-e".to_string(),
                            flag: "--allow-empty".to_string(),
                            label: "Allow empty commit".to_string(),
                            on: false,
                            status: MenuStatus::implemented(),
                        },
                        Item::Switch {
                            key: "-n".to_string(),
                            flag: "--no-verify".to_string(),
                            label: "Bypass commit hooks".to_string(),
                            on: false,
                            status: MenuStatus::implemented(),
                        },
                        Item::Option {
                            key: "=A".to_string(),
                            flag: "--author=".to_string(),
                            label: "Override the author".to_string(),
                            value: None,
                            reader: Reader::Text,
                            status: MenuStatus::implemented(),
                        },
                    ],
                ),
                Group::new(
                    "Create",
                    vec![
                        action("c", "commit.create", "Commit", "R-4.1"),
                        action("e", "commit.extend", "Extend", "R-4.1"),
                        action("f", "commit.fixup", "Fixup", "R-4.1"),
                        action("s", "commit.squash", "Squash", "R-4.1"),
                        action("i", "commit.instant-fixup", "Instant fixup", "R-4.1"),
                        action("I", "commit.instant-squash", "Instant squash", "R-4.1"),
                    ],
                ),
                Group::new(
                    "Edit HEAD",
                    vec![
                        action("a", "commit.amend", "Amend", "R-4.1"),
                        action("w", "commit.reword", "Reword", "R-4.1"),
                        action("A", "commit.augment", "Augment", "R-4.1"),
                        action("v", "commit.absorb", "Absorb", "R-4.1"),
                    ],
                ),
                Group::new(
                    "Rearrange",
                    vec![
                        action("r", "commit.reorder", "Reorder commits", "R-4.1"),
                        action("m", "commit.move", "Move commit", "R-4.1"),
                        action("x", "commit.drop", "Drop the selected commit", "R-4.1"),
                    ],
                ),
                Group::new(
                    "Menu",
                    vec![
                        Item::Action {
                            key: "?".to_string(),
                            command: "open.dispatch".to_string(),
                            label: "Show the dispatch menu".to_string(),
                            status: MenuStatus::implemented(),
                        },
                        action("l", "log.recent", "Show recent commits", "R-4.3"),
                        action("d", "diff.staged", "Show staged diff", "R-4.2"),
                        action("C", "commit.configure", "Configure commit options and arguments", "R-4.1"),
                    ],
                ),
            ],
        }
    }

    /// A small menu pushed from the commit menu, proving the menu stack.
    pub fn dispatch() -> Self {
        Self {
            id: "dispatch".to_string(),
            title: "Dispatch".to_string(),
            saved_args: Args::default(),
            groups: vec![
                Group::new(
                    "Create",
                    vec![
                        action("c", "open.commit", "Commit", "R-2.8"),
                        action("b", "open.branch", "Branch", "R-2.8"),
                        action("t", "open.tag", "Tag", "R-2.8"),
                    ],
                ),
                Group::new(
                    "Change",
                    vec![
                        action("f", "open.fetch", "Fetch", "R-2.8"),
                        action("P", "open.push", "Push", "R-2.8"),
                        action("m", "open.merge", "Merge", "R-2.8"),
                    ],
                ),
            ],
        }
    }

    /// The menu with this name, if it ships today.
    pub fn by_name(name: &str) -> Option<Self> {
        match name {
            "commit" => Some(Self::commit()),
            "dispatch" => Some(Self::dispatch()),
            _ => None,
        }
    }

    /// The command the argument preview starts with.
    fn base_command(&self) -> String {
        match self.id.as_str() {
            "commit" => "git commit".to_string(),
            other => format!("magit-lsp {other}"),
        }
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
                if let Item::Switch { key: item_key, on, .. } = item {
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
                    Item::Switch {
                        flag, on: true, ..
                    } => parts.push(flag.clone()),
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
        let mut builder = Builder::new(self);
        builder.title();
        builder.arguments();
        builder.preview();
        builder.actions(viewport.cols as usize);
        builder.finish()
    }
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

    fn render(&mut self, ctx: &Ctx) -> Tree {
        self.menu.render(ctx.viewport)
    }

    fn menu(&self) -> Option<Menu> {
        Some(self.menu.clone())
    }

    fn menu_mut(&mut self) -> Option<&mut Menu> {
        Some(&mut self.menu)
    }
}

fn action(key: &str, command: &str, label: &str, roadmap_id: &str) -> Item {
    Item::Action {
        key: key.to_string(),
        command: command.to_string(),
        label: label.to_string(),
        status: MenuStatus::planned(roadmap_id),
    }
}

/// The text and structure builder. Keeping layout in one place makes the
/// column reflow on `view/resize` a pure function of the viewport.
struct Builder<'a> {
    menu: &'a Menu,
    tree: Tree,
}

impl<'a> Builder<'a> {
    fn new(menu: &'a Menu) -> Self {
        Self {
            menu,
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
        self.tree
            .line_nodes
            .push(line_node(&id, NodeKind::Header));
        self.tree
            .nodes
            .push(node(&id, NodeKind::Header, self.menu.title.clone(), line));
        self.token(line, 1, self.menu.title.len(), "magit.menu.title");
    }

    /// The `Arguments` group is vertical: one switch or option per line.
    fn arguments(&mut self) {
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
                children.push(self.argument_item(item));
            }
            group_node.children = children;
            group_node.end_line = self.tree.lines.len().saturating_sub(1);
            self.tree.nodes[index] = group_node;
        }
    }

    fn argument_item(&mut self, item: &Item) -> String {
        let key = item.key();
        let label = item.label();
        let id = format!("menu:{}/item:{key}", self.menu.id);
        let line = self.tree.lines.len();
        let mut text = format!("  {key}  {label}");
        let key_type = if item.is_option() {
            "magit.menu.option"
        } else {
            "magit.menu.key"
        };
        self.token(line, 2, key.len(), key_type);
        let label_start = 2 + key.len() + 2;
        self.token(line, label_start, label.len(), "magit.menu.label");

        if let Item::Switch { flag, on, .. } = item {
            text.push_str(&format!(" ({flag})"));
            let flag_start = label_start + label.len() + 2;
            self.token(line, flag_start, flag.len(), "magit.menu.flag");
            text.push_str("  ");
            let marker = if *on { "[x]" } else { "[ ]" };
            let marker_start = text.len();
            text.push_str(marker);
            self.token(
                line,
                marker_start,
                3,
                if *on {
                    "magit.menu.switch.on"
                } else {
                    "magit.menu.switch.off"
                },
            );
        } else if let Item::Option { flag, value, .. } = item {
            text.push_str(&format!(" ({flag})"));
            let flag_start = label_start + label.len() + 2;
            self.token(line, flag_start, flag.len(), "magit.menu.flag");
            if let Some(value) = value {
                text.push_str("  ");
                let value_start = text.len();
                text.push_str(value);
                self.token(line, value_start, value.len(), "magit.menu.value");
            }
        }

        self.tree.lines.push(text);
        self.item_nodes(&id, item, line);
        id
    }

    /// The preview line follows the arguments it describes.
    fn preview(&mut self) {
        if !self.menu.groups.iter().any(|group| group.title == "Arguments") {
            return;
        }
        let line = self.tree.lines.len();
        let text = self.menu.preview();
        let id = format!("menu:{}/preview", self.menu.id);
        self.tree.lines.push(format!(" {text}"));
        self.tree
            .line_nodes
            .push(line_node(&id, NodeKind::Message));
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
                            self.token(
                                line,
                                base + 1,
                                block.group.title.len(),
                                "magit.menu.group",
                            );
                        } else {
                            let item = &block.group.items[line_offset - 1];
                            let key = item.key();
                            let label = item.label();
                            tokens.push((base + 2, key.len(), key_token(item)));
                            tokens.push((base + 2 + key.len() + 2, label.len(), label_token(item)));
                            owner = Some((key, item));
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
                        self.tree.line_nodes.push(line_node(&group_id, NodeKind::Section));
                    }
                }
            }
        }
    }

    fn item_nodes(&mut self, id: &str, item: &Item, line: usize) {
        let mut entry = line_node(id, NodeKind::Line);
        entry.status = Some(status_name(item.status()));
        self.tree.line_nodes.push(entry);
        self.tree.nodes.push(item_node(id, item, line));
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
        MenuStatus::Unavailable { .. } => "magit.menu.disabled",
    }
}

fn status_name(status: &MenuStatus) -> String {
    match status {
        MenuStatus::Implemented => "implemented".to_string(),
        MenuStatus::Planned { roadmap_id } => format!("planned:{roadmap_id}"),
        MenuStatus::Unavailable { .. } => "unavailable".to_string(),
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
        children: Vec::new(),
    }
}

fn item_node(id: &str, item: &Item, line: usize) -> Node {
    let mut node = node(id, NodeKind::Line, item.label().to_string(), line);
    node.status = Some(status_name(item.status()));
    node.copy = Some(item.label().to_string());
    node
}
