// Release builds on Windows get no console window; use `markdraft-cli` for
// headless commands there (see DECISIONS.md).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> anyhow::Result<()> {
    let cli = markdraft_lib::parse_cli(std::env::args_os());
    if cli::run_headless(&cli)? {
        return Ok(());
    }
    markdraft_lib::run(cli.path)
}
