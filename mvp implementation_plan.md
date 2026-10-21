---

# Majjit: LSP-Based Version Control Interface (Git & Neovim MVP)

## 1. Project Context & Architectural Summary

**The Problem:** Traditional version control system (VCS) interfaces are either deeply coupled to specific editors (requiring massive duplicated effort, like Magit for Emacs and edamagit for VSCode) or relegated to terminal user interfaces (TUIs) that break editor immersion and window management.
**The Solution:** "Majjit" abstracts the VCS UI into a Language Server Protocol (LSP) implementation. By treating the repository status as a dynamic, structured text document, we can leverage standard LSP features (folding ranges, semantic tokens, code actions) to provide a rich, interactive VCS interface inside any modern editor natively.
**The MVP Scope:** We are building the initial Minimum Viable Product (MVP) using Git as the backend and Neovim as the client.

* **Git Backend:** Provides a direct baseline to benchmark feature parity and UI responsiveness against existing tools like Magit or Neogit. We will use the pure-Rust `gitoxide` (`gix`) crate to ensure non-blocking, multi-threaded repository traversal.


* **Neovim Client:** Neovim's native Lua LSP client allows for rapid prototyping of virtual documents and custom protocol extensions without the overhead of packaging heavy webviews.


* **Server-Driven UI (SDUI):** Because standard LSP cannot render Magit's multi-stage, interactive transient menus, the server will implement a custom SDUI extension to send JSON menu schemas to Neovim, which will render them as floating windows.



## 2. LSP API Requirements & Implementation Strategy

To successfully trick the editor into treating a Git repository as a "language," the agent must correctly implement specific phases of the LSP lifecycle. The LSP is entirely push-based from the editor's perspective; the editor never analyzes code, it simply forwards state changes and renders the server's responses.

### A. The Initialization Handshake

When Neovim connects, it sends an `initialize` request. The Rust server must respond with an `InitializeResult` declaring its capabilities.

* **Strategy:** The server must declare `TextDocumentSyncKind::FULL` (or `INCREMENTAL`) to tell the editor to send document updates. It must also declare custom capabilities (e.g., `experimental.transientMenuProvider = true`) so the Lua client knows the server supports the custom SDUI endpoints.



### B. Virtual Document URIs & Synchronization

Writing the status buffer to a physical disk file (like `.jj/status`) is a major anti-pattern. Modern editors attach file watchers to physical files. Rapid mutations by the Language Server will trigger a cascade of `workspace/didChangeWatchedFiles` events, leading to severe CPU spikes and infinite feedback loops.

* **Strategy:** The Neovim client must create an immutable, memory-only buffer (using `buftype="nofile"`) and attach the LSP client using a custom URI scheme (e.g., `vcs://status` or `untitled://status`).


* **Sync Logic:** When the buffer opens, Neovim sends `textDocument/didOpen`. The server uses this to initialize its in-memory state. As the user triggers actions, the server computes the new diff and sends a `workspace/applyEdit` request back to Neovim to redraw the UI instantly without disk I/O.



### C. Custom Protocol Extensions (SDUI)

Standard LSP `textDocument/codeAction` requests only return flat arrays of clickable options. They cannot handle Magit's keystroke-bound, toggleable transient menus.

* **Strategy:** Extend the JSON-RPC protocol.
1. The Neovim client intercepts a keystroke (e.g., `c` for commit).
2. Neovim sends a custom `vcs/requestTransient` JSON-RPC request.
3. The server responds with a JSON schema defining available flags (e.g., `--amend`), their states, and executable actions.
4. Neovim draws this as a floating window. When the user finalizes the command, Neovim sends a standard `workspace/executeCommand`, prompting the server to mutate the Git repo and push a `workspace/applyEdit` to refresh the main buffer.



## 3. Verified Core Dependencies

The agent must use the following established libraries:

**Rust (LSP Server):**

* `lsp-server` (crates.io/crates/lsp-server): The industry-standard boilerplate for stdio threading and JSON-RPC parsing.
* `lsp-types` (crates.io/crates/lsp-types): Strongly-typed Rust structs for LSP 3.17+ primitives (e.g., `FoldingRange`, `SemanticToken`).


* `gix` (crates.io/crates/gix): A pure-Rust, thread-safe Git implementation required for background LSP tasks.



**Lua (Neovim Client & Testing):**

* `mini.test` ([github.com/echasnovski/mini.test](https://github.com/echasnovski/mini.test)): The modern standard for Neovim plugin testing, executing tests in clean, isolated child processes.



---

## 4. Step-by-Step Implementation Blueprint

### Step 1: Rust Project Initialization (`Cargo.toml`)

Initialize a new Rust binary. Configure the dependencies required for the server.

```toml
[package]
name = "majjit-server"
version = "0.1.0"
edition = "2021"

[dependencies]
lsp-server = "0.7"
lsp-types = "0.94"
gix = "0.57"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
crossbeam-channel = "0.5"

```

### Step 2: Protocol Extensions & SDUI Schemas (Rust)

Create `src/protocol.rs`. This defines the custom JSON-RPC request for the transient menus. It manually implements the `lsp_types::request::Request` trait so it integrates cleanly with `lsp-server`.

```rust
use serde::{Deserialize, Serialize};

pub enum TransientMenuRequest {}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct TransientMenuParams {
    pub trigger_key: String,
    pub document_uri: String,
    pub cursor_line: u32,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct TransientMenuResponse {
    pub title: String,
    pub flags: Vec<TransientFlag>,
    pub actions: Vec<TransientAction>,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct TransientFlag {
    pub flag: String,
    pub description: String,
    pub active: bool,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct TransientAction {
    pub key: String,
    pub command: String,
    pub description: String,
}

impl lsp_types::request::Request for TransientMenuRequest {
    type Params = TransientMenuParams;
    type Result = TransientMenuResponse;
    const METHOD: &'static str = "vcs/requestTransient";
}

```

### Step 3: Git Abstraction Layer (Rust)

Create `src/vcs.rs`. Use `gix` for non-blocking local reads (diffs, status). Note that mutations later on should rely on shelling out to the `git` CLI subprocess to avoid `index.lock` collisions and complex authentication edge cases.

```rust
use gix::ThreadSafeRepository;
use std::path::Path;

pub trait VcsBackend {
    fn get_status_text(&self) -> Result<String, String>;
    fn execute_mutation(&self, command: &str, args: Vec<&str>) -> Result<(), String>;
}

pub struct GitoxideBackend {
    repo: ThreadSafeRepository,
}

impl GitoxideBackend {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let repo = gix::discover(path).map_err(|e| e.to_string())?;
        Ok(Self { repo: repo.into_sync() })
    }
}

impl VcsBackend for GitoxideBackend {
    fn get_status_text(&self) -> Result<String, String> {
        // Stub: Convert gix status into Magit-style structured text string
        let _repo = self.repo.to_thread_local();
        Ok("Head: main\n\nUnstaged changes:\n- modified file.rs\n".to_string())
    }

    fn execute_mutation(&self, _command: &str, _args: Vec<&str>) -> Result<(), String> {
        // Stub: Shell out to `git` CLI subprocess for mutations
        Ok(())
    }
}

```

### Step 4: LSP Server Main Loop (Rust)

Create `src/main.rs`. Set up the stdio IPC connection and broadcast both standard text sync capabilities and the custom SDUI capability.

```rust
use lsp_server::{Connection, Message, Request, Response};
use lsp_types::{
    ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, ExecuteCommandOptions
};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error + Sync + Send>> {
    let (connection, io_threads) = Connection::stdio();

    let server_capabilities = serde_json::to_value(&ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Options(TextDocumentSyncOptions {
            open_close: Some(true),
            change: Some(TextDocumentSyncKind::FULL),
            ..Default::default()
        })),
        execute_command_provider: Some(ExecuteCommandOptions {
            commands: vec!["majjit.executeVCSCommand".to_string()],
            ..Default::default()
        }),
        // Broadcast custom SDUI support
        experimental: Some(serde_json::json!({
            "transientMenuProvider": true
        })),
        ..Default::default()
    }).unwrap();

    let _initialization_params = connection.initialize(server_capabilities)?;

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? { return Ok(()); }
                handle_request(&connection, req)?;
            }
            Message::Response(_resp) => { /* Handle client responses */ }
            Message::Notification(_not) => { /* Handle didOpen, didChange */ }
        }
    }
    io_threads.join()?;
    Ok(())
}

fn handle_request(connection: &Connection, req: Request) -> Result<(), Box<dyn Error + Sync + Send>> {
    if req.method == "vcs/requestTransient" {
        // Return the custom SDUI JSON schema
        let result = serde_json::json!({
            "title": "Commit Menu",
            "flags": [
                { "flag": "--amend", "description": "Amend commit", "active": false }
            ],
            "actions": [
                { "key": "c", "command": "commit", "description": "Create commit" }
            ]
        });
        let resp = Response::new_ok(req.id, result);
        connection.sender.send(Message::Response(resp))?;
    }
    Ok(())
}

```

### Step 5: Neovim Client Implementation (Lua)

Create `lua/majjit/init.lua`. This uses the built-in Neovim `vim.lsp.start` API to boot the binary. Crucially, it sets `buftype="nofile"` to prevent disk writes, ensuring we maintain a virtual document architecture.

```lua
local M = {}

function M.setup()
    -- Start the LSP server binary
    local client_id = vim.lsp.start_client({
        name = 'majjit-server',
        cmd = { 'cargo', 'run', '--manifest-path', vim.fn.getcwd() .. '/Cargo.toml' },
        root_dir = vim.fn.getcwd(),
    })

    if not client_id then
        vim.notify("Failed to start Majjit LSP", vim.log.levels.ERROR)
        return
    end

    -- Create an immutable virtual buffer (No disk I/O, prevents file watcher storms)
    local buf = vim.api.nvim_create_buf(false, true)
    vim.bo[buf].buftype = "nofile"
    vim.bo[buf].bufhidden = "hide"
    vim.bo[buf].swapfile = false
    -- Set a custom URI via buffer name
    vim.api.nvim_buf_set_name(buf, "vcs://status")

    vim.lsp.buf_attach_client(buf, client_id)

    -- Intercept 'c' to request the Transient Menu via SDUI instead of standard CodeAction
    vim.keymap.set('n', 'c', function()
        M.request_transient(buf, 'c')
    end, { buffer = buf, desc = "Majjit Commit Menu" })

    -- Open the UI split
    vim.cmd('vsplit')
    vim.api.nvim_win_set_buf(0, buf)
end

function M.request_transient(buf, key)
    local params = {
        trigger_key = key,
        document_uri = vim.uri_from_bufnr(buf),
        cursor_line = vim.fn.line('.') - 1,
    }

    -- Dispatch custom JSON-RPC request to the Rust server
    vim.lsp.buf_request(buf, 'vcs/requestTransient', params, function(err, result)
        if err then vim.notify(err.message, vim.log.levels.ERROR) return end

        -- MVP Stub: Print the parsed JSON schema.
        -- Next step: Pass this `result` table to vim.api.nvim_open_win to draw the popup.
        print("Received SDUI Schema: " .. vim.inspect(result))
    end)
end

return M

```

### Step 6: Testing Setup with `mini.test`

Create `tests/test_majjit.lua`. `mini.test` will spawn a headless child Neovim process to ensure the buffer settings and LSP attachment occur correctly in complete isolation.

```lua
-- tests/test_majjit.lua
local MiniTest = require('mini.test')
local T = MiniTest.new_set()

-- Spin up a fresh child process for these tests
local child = MiniTest.new_child_neovim()

T.setup = function()
  child.restart({ '-u', 'scripts/minimal_init.lua' })
  -- Optionally load your plugin path into the child process here
end

T.teardown = function()
  child.stop()
end

T['creates virtual buffer and attaches LSP'] = function()
  -- Execute the setup function inside the child process
  child.lua([[
    require('majjit').setup()
  ]])

  -- Verify the active buffer is correctly configured as a virtual document
  local buftype = child.api.nvim_get_option_value('buftype', { buf = 0 })
  MiniTest.expect.equality(buftype, 'nofile')

  local bufname = child.api.nvim_buf_get_name(0)
  MiniTest.expect.equality(string.match(bufname, "^vcs://"), "vcs://")
end

return T

```
