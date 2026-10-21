fn main() {
    if let Err(e) = magit_lsp::server::run() {
        eprintln!("magit-lsp: {e}");
        std::process::exit(1);
    }
}