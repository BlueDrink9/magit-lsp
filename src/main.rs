/// Entry point: logging first, then the stdio LSP server.
fn main() {
    if let Err(e) = magit_lsp::logging::init() {
        eprintln!("magit-lsp: could not initialise logging: {e}");
    }

    if let Err(e) = magit_lsp::server::run() {
        tracing::error!(error = %e, "server stopped");
        // A fatal startup error still needs one readable line on stderr.
        eprintln!("magit-lsp: {e}");
        std::process::exit(1);
    }
}
