/// Entry point: logging first, then the stdio LSP server.
fn main() {
    if let Err(e) = magit_lsp::logging::init() {
        eprintln!("magit-lsp: could not initialise logging: {e}");
    }

    let options = parse_args(std::env::args().skip(1));
    if let Err(e) = magit_lsp::server::run_with(options) {
        tracing::error!(error = %e, "server stopped");
        // A fatal startup error still needs one readable line on stderr.
        eprintln!("magit-lsp: {e}");
        std::process::exit(1);
    }
}

/// Parse the small command line: only `--config <path>` is meaningful today.
fn parse_args(args: impl Iterator<Item = String>) -> magit_lsp::server::ServerOptions {
    let mut options = magit_lsp::server::ServerOptions::default();
    let mut args = args;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => options.config_path = args.next().map(std::path::PathBuf::from),
            other if other.starts_with("--config=") => {
                options.config_path = Some(std::path::PathBuf::from(&other["--config=".len()..]));
            }
            _ => {}
        }
    }
    options
}
