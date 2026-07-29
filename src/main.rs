#![doc = include_str!("../README.md")]
//! expman CLI: friendly command-line interface for experiment management.

/// Restore the default `SIGPIPE` disposition.
///
/// Rust ignores SIGPIPE at startup, which turns a closed downstream pipe into
/// an `EPIPE` write error — and `println!` panics on those. The visible effect
/// is that `exp list | head` prints its output and *then* a panic with a
/// backtrace. For a tool people pipe into `head`, `less` and `grep` all day
/// that is simply wrong; with the default handler the process exits quietly,
/// as every other CLI does.
#[cfg(unix)]
fn restore_sigpipe() {
    // SAFETY: sets a signal disposition before any thread is spawned, which is
    // the documented-safe window for this call.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn restore_sigpipe() {}

#[tokio::main]
async fn main() {
    restore_sigpipe();
    expman::cli::init_tracing();
    if let Err(e) = expman::cli::run_cli().await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
