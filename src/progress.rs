use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};

/// Options controlling progress display and interactivity.
#[derive(Debug, Clone, Copy)]
pub struct ProgressOpts {
    pub non_interactive: bool,
}

/// Context for batch operations that need both progress display and
/// journal support (crash recovery via `.zuul/journal.json`).
#[derive(Debug, Clone)]
pub struct BatchContext {
    pub progress: ProgressOpts,
    pub project_root: Option<PathBuf>,
}

impl BatchContext {
    /// Convenience accessor for the project root as a `&Path`.
    pub fn root(&self) -> Option<&Path> {
        self.project_root.as_deref()
    }
}

impl ProgressOpts {
    /// Returns `true` if progress indicators should be hidden.
    fn is_hidden(self) -> bool {
        self.non_interactive || !std::io::stderr().is_terminal()
    }
}

/// Create a spinner with a message, respecting non-interactive/pipe modes.
///
/// Returns a [`ProgressBar`] that either renders a spinner to stderr or is hidden.
/// The caller should call `.finish_and_clear()` or `.finish_with_message()` when done.
pub fn spinner(message: &str, opts: ProgressOpts) -> ProgressBar {
    if opts.is_hidden() {
        return ProgressBar::hidden();
    }

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .expect("invalid spinner template"),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

/// Create a progress bar for counted operations (e.g., import).
///
/// Returns a [`ProgressBar`] that either renders a progress bar to stderr or is hidden.
pub fn progress_bar(total: u64, opts: ProgressOpts) -> ProgressBar {
    if opts.is_hidden() {
        return ProgressBar::hidden();
    }

    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.cyan} [{pos}/{len}] {msg}")
            .expect("invalid progress bar template"),
    );
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}
