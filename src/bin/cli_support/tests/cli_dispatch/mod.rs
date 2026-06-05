//! CLI dispatch smoke tests.
//!
//! Pin clap-level parsing for shipped subcommands without invoking runtime.

use clap::Parser;

use super::super::cli_args::Cli;

pub(super) fn parse(args: &[&str]) -> Cli {
    let mut full = vec!["synrepo"];
    full.extend_from_slice(args);
    Cli::try_parse_from(full).expect("args should parse")
}

mod context;
mod core;
mod global_flags;
mod notes_lessons;
mod project_watch;
