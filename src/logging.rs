//! Logging setup: stderr or a file, never stdout.
//!
//! stdout is the LSP pipe, so a stray log line there corrupts the protocol.

use std::io;
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::fmt::writer::BoxMakeWriter;

/// Environment variable holding an `EnvFilter` directive for the log level.
pub const LEVEL_ENV: &str = "MAGIT_LSP_LOG";

/// Environment variable naming a file to receive logs instead of stderr.
pub const FILE_ENV: &str = "MAGIT_LSP_LOG_FILE";

/// Level used when [`LEVEL_ENV`] is unset or not a valid directive.
pub const DEFAULT_LEVEL: &str = "warn";

/// Install the global subscriber, writing to stderr unless [`FILE_ENV`] is set.
///
/// Returns the I/O error when the log file cannot be opened, so startup can
/// report it instead of silently dropping logs.
pub fn init() -> io::Result<()> {
    let filter =
        EnvFilter::try_from_env(LEVEL_ENV).unwrap_or_else(|_| EnvFilter::new(DEFAULT_LEVEL));

    let writer: BoxMakeWriter = match std::env::var_os(FILE_ENV) {
        Some(path) => BoxMakeWriter::new(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)?,
        ),
        None => BoxMakeWriter::new(io::stderr),
    };

    // No ANSI escapes: logs may end up in a file or a client's log pane.
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(false)
        .try_init()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
}
