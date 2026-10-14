// Release builds on Windows get no console window; headless commands
// borrow the calling terminal's console instead (see DECISIONS.md).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> anyhow::Result<()> {
    // Run by the Windows setup and uninstaller: the Settings > System options.
    match std::env::args_os()
        .nth(1)
        .as_deref()
        .and_then(|a| a.to_str())
    {
        Some("--install-integration") => {
            markdraft_lib::integration::install_all();
            return Ok(());
        }
        Some("--remove-integration") => {
            markdraft_lib::integration::remove_all();
            return Ok(());
        }
        _ => {}
    }
    let cli = markdraft_lib::parse_cli(std::env::args_os());
    #[cfg(windows)]
    if cli.command.is_some() {
        markdraft_lib::win32::attach_parent_console();
    }
    if cli::run_headless(&cli)? {
        return Ok(());
    }
    markdraft_lib::run(cli.path)
}
