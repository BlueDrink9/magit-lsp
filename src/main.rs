// The LSP wire protocol owns stdout; a stray print would corrupt it.
#![deny(clippy::print_stdout)]
#![deny(clippy::dbg_macro)]

/// Entry point: dispatch the `keymap` subcommand or run the stdio LSP server.
fn main() {
    // The writers are dropped before the server starts so the server owns
    // stdout for framed JSON-RPC.
    let outcome = {
        let mut stdout = std::io::stdout();
        let mut stderr = std::io::stderr();
        magit_lsp::cli::run(std::env::args().skip(1), &mut stdout, &mut stderr)
    };

    let options = match outcome {
        magit_lsp::cli::Outcome::Done(code) => std::process::exit(code),
        magit_lsp::cli::Outcome::Server(options) => options,
    };

    if let Err(e) = magit_lsp::logging::init() {
        eprintln!("magit-lsp: could not initialise logging: {e}");
    }

    if let Err(e) = magit_lsp::server::run_with(options) {
        tracing::error!(error = %e, "server stopped");
        // A fatal startup error still needs one readable line on stderr.
        eprintln!("magit-lsp: {e}");
        std::process::exit(1);
    }
}
