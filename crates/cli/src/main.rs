//! `markdraft-cli`: headless entry point for scripts and CI.

use clap::Parser;

fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();
    if !cli::run_headless(&cli)? {
        anyhow::bail!(
            "markdraft-cli only runs headless commands; use the MarkDraft app to open files"
        );
    }
    Ok(())
}
