//! Command line: the stdio server by default, plus the `keymap` subcommand.
//!
//! Subcommand output goes through [`run`] so there is exactly one place that
//! writes to stdout. The server path never uses this module's writers.

use crate::config::{self, LoadOptions, Profile};
use crate::manifest::{self, Format};
use crate::server::ServerOptions;
use std::io::Write;
use std::path::PathBuf;

/// What the process should do after the command line is parsed.
pub enum Outcome {
    /// Run the LSP server on stdio with these options.
    Server(ServerOptions),
    /// A subcommand ran; exit with this status.
    Done(i32),
}

const USAGE: &str = "\
Usage: magit-lsp [--config <path>]
       magit-lsp keymap [--format json|vscode|neovim|markdown] [--profile standard|vim] [--config <path>] [--check]
       magit-lsp parity [--format markdown|json]
       magit-lsp --version | --help

With no subcommand, run the LSP server on stdio. The keymap and parity
subcommands print generated tables and never speak LSP.
";

const PARITY_USAGE: &str = "\
Usage: magit-lsp parity [--format markdown|json]

Print the Magit command parity table. `cargo xtask parity` is the same
command through the cargo alias.
";

const KEYMAP_USAGE: &str = "\
Usage: magit-lsp keymap [--format json|vscode|neovim|markdown] [--profile standard|vim] [--config <path>] [--check]

Without --profile, emit every shipped profile. --check validates the merged
keymap and prints OK; combine it with --format to emit only when valid.
";

/// Parse the command line and run any subcommand, writing to `out` and `err`.
pub fn run(
    args: impl Iterator<Item = String>,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Outcome {
    let outcome = dispatch(args.collect(), out, err);
    let _ = out.flush();
    let _ = err.flush();
    outcome
}

fn dispatch(args: Vec<String>, out: &mut impl Write, err: &mut impl Write) -> Outcome {
    match args.first().map(String::as_str) {
        None => Outcome::Server(ServerOptions::default()),
        Some("keymap") => run_keymap(&args[1..], out, err),
        Some("parity") => run_parity(&args[1..], out, err),
        Some("--version") | Some("-V") => {
            let _ = writeln!(out, "magit-lsp {}", env!("CARGO_PKG_VERSION"));
            Outcome::Done(0)
        }
        Some("--help") | Some("-h") => {
            let _ = write!(out, "{USAGE}");
            Outcome::Done(0)
        }
        Some(flag) if flag.starts_with('-') => match parse_server_args(&args) {
            Ok(options) => Outcome::Server(options),
            Err(message) => {
                let _ = writeln!(err, "{message}\n\n{USAGE}");
                Outcome::Done(2)
            }
        },
        Some(other) => {
            let _ = writeln!(err, "unknown subcommand `{other}`\n\n{USAGE}");
            Outcome::Done(2)
        }
    }
}

/// The server accepts only `--config`; anything else is a usage error.
fn parse_server_args(args: &[String]) -> Result<ServerOptions, String> {
    let mut options = ServerOptions::default();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--config" {
            let value = iter.next().ok_or("`--config` needs a path")?;
            options.config_path = Some(PathBuf::from(value));
        } else if let Some(value) = arg.strip_prefix("--config=") {
            options.config_path = Some(PathBuf::from(value));
        } else {
            return Err(format!("unknown option `{arg}`"));
        }
    }
    Ok(options)
}

struct KeymapArgs {
    format: Option<Format>,
    profile: Option<Profile>,
    config_path: Option<PathBuf>,
    check: bool,
}

fn parse_keymap_args(args: &[String]) -> Result<KeymapArgs, String> {
    let mut parsed = KeymapArgs {
        format: None,
        profile: None,
        config_path: None,
        check: false,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--check" {
            parsed.check = true;
        } else if arg == "--format" {
            let value = iter.next().ok_or("`--format` needs a value")?;
            parsed.format = Some(Format::parse(value)?);
        } else if let Some(value) = arg.strip_prefix("--format=") {
            parsed.format = Some(Format::parse(value)?);
        } else if arg == "--profile" {
            let value = iter.next().ok_or("`--profile` needs a value")?;
            parsed.profile = Some(Profile::parse(value)?);
        } else if let Some(value) = arg.strip_prefix("--profile=") {
            parsed.profile = Some(Profile::parse(value)?);
        } else if arg == "--config" {
            let value = iter.next().ok_or("`--config` needs a path")?;
            parsed.config_path = Some(PathBuf::from(value));
        } else if let Some(value) = arg.strip_prefix("--config=") {
            parsed.config_path = Some(PathBuf::from(value));
        } else {
            return Err(format!("unknown option `{arg}`"));
        }
    }
    Ok(parsed)
}

fn run_keymap(args: &[String], out: &mut impl Write, err: &mut impl Write) -> Outcome {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        let _ = write!(out, "{KEYMAP_USAGE}");
        return Outcome::Done(0);
    }
    let parsed = match parse_keymap_args(args) {
        Ok(parsed) => parsed,
        Err(message) => {
            let _ = writeln!(err, "{message}\n\n{KEYMAP_USAGE}");
            return Outcome::Done(2);
        }
    };

    let profiles = match parsed.profile {
        Some(profile) => vec![profile],
        None => vec![Profile::Standard, Profile::Vim],
    };

    let options = LoadOptions {
        config_path: parsed.config_path,
        ..Default::default()
    };
    let config = match config::load_config(&options) {
        Ok(config) => config,
        Err(error) => {
            let _ = writeln!(err, "{error}");
            return Outcome::Done(1);
        }
    };

    if parsed.check {
        if let Err(error) = manifest::check(&profiles, &config) {
            let _ = writeln!(err, "{error}");
            return Outcome::Done(1);
        }
        // `--check` alone is a validator; with an explicit `--format` it gates
        // the emission.
        if parsed.format.is_none() {
            let _ = writeln!(out, "OK");
            return Outcome::Done(0);
        }
    }

    let format = parsed.format.unwrap_or(Format::Json);
    if let Err(error) = manifest::render(format, &profiles, &config, out) {
        let _ = writeln!(err, "{error}");
        return Outcome::Done(1);
    }
    Outcome::Done(0)
}

/// Print the Magit parity table.
fn run_parity(args: &[String], out: &mut impl Write, err: &mut impl Write) -> Outcome {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        let _ = write!(out, "{PARITY_USAGE}");
        return Outcome::Done(0);
    }
    let mut format = "markdown";
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--format" {
            match iter.next() {
                Some(value) => format = value,
                None => {
                    let _ = writeln!(err, "`--format` needs a value\n\n{PARITY_USAGE}");
                    return Outcome::Done(2);
                }
            }
        } else if let Some(value) = arg.strip_prefix("--format=") {
            format = value;
        } else {
            let _ = writeln!(err, "unknown option `{arg}`\n\n{PARITY_USAGE}");
            return Outcome::Done(2);
        }
    }
    let text = match format {
        "markdown" => crate::parity::markdown(),
        "json" => crate::parity::json(),
        other => {
            let _ = writeln!(
                err,
                "unknown format `{other}`; expected markdown or json\n\n{PARITY_USAGE}"
            );
            return Outcome::Done(2);
        }
    };
    let _ = out.write_all(text.as_bytes());
    Outcome::Done(0)
}
