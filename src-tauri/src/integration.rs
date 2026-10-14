//! Optional OS integration, off by default and toggled from Settings:
//!
//! * a `markdraft` command on the user's PATH (`markdraft` alone opens the
//!   current folder, `markdraft notes/` opens a folder, `markdraft export …`
//!   runs the headless exporter);
//! * "Open with MarkDraft" on folders and "Open MarkDraft here" on folder
//!   backgrounds in the file manager.
//!
//! Everything is per-user, needs no admin rights and is fully reversible.
//! The Windows setup turns both on after asking (`--install-integration`);
//! the MSI sets them up machine-wide itself, which shows here as "managed".
//! Generated launchers and menu entries point at the running executable, so
//! a moved app shows up as "stale" until the toggle is flipped again.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Feature {
    pub supported: bool,
    pub enabled: bool,
    /// Installed, but for another copy of the app.
    pub stale: bool,
    /// Set up machine-wide by an installer; changing it needs an administrator.
    pub managed: bool,
    /// Where it was installed, or what the user still has to do.
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationStatus {
    pub cli: Feature,
    pub context_menu: Feature,
}

/// The executable the integration should launch.
pub fn app_exe() -> io::Result<PathBuf> {
    // An AppImage runs from a temporary mount; point at the image itself.
    if let Some(image) = std::env::var_os("APPIMAGE") {
        return Ok(PathBuf::from(image));
    }
    let exe = std::env::current_exe()?;
    Ok(dunce::canonicalize(&exe).unwrap_or(exe))
}

pub fn status(exe: &Path) -> IntegrationStatus {
    IntegrationStatus {
        cli: platform::cli_status(exe),
        context_menu: platform::menu_status(exe),
    }
}

pub fn set_cli(exe: &Path, enable: bool) -> io::Result<Feature> {
    if enable {
        platform::install_cli(exe)?;
    } else {
        platform::remove_cli(exe)?;
    }
    Ok(platform::cli_status(exe))
}

pub fn set_context_menu(exe: &Path, enable: bool) -> io::Result<Feature> {
    if enable {
        platform::install_menu(exe)?;
    } else {
        platform::remove_menu()?;
    }
    Ok(platform::menu_status(exe))
}

/// Sets everything up (run by the Windows setup via `--install-integration`).
pub fn install_all() {
    let exe = match app_exe() {
        Ok(exe) => exe,
        Err(e) => return tracing::warn!("cannot find the app: {e}"),
    };
    if let Err(e) = platform::install_cli(&exe) {
        tracing::warn!("could not add the markdraft command: {e}");
    }
    if let Err(e) = platform::install_menu(&exe) {
        tracing::warn!("could not add the folder menu entries: {e}");
    }
}

/// Removes everything (run by the uninstaller via `--remove-integration`).
pub fn remove_all() {
    let exe = app_exe().unwrap_or_default();
    if let Err(e) = platform::remove_cli(&exe) {
        tracing::warn!("could not remove the markdraft command: {e}");
    }
    if let Err(e) = platform::remove_menu() {
        tracing::warn!("could not remove the folder menu entries: {e}");
    }
}

/// Deletes a file, treating "already gone" as success.
fn remove_file(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Whether a file we wrote exists, and whether it still names `exe`.
fn file_state(path: &Path, exe: &Path) -> (bool, bool) {
    match fs::read_to_string(path) {
        Ok(text) => (true, !text.contains(&*exe.to_string_lossy())),
        Err(_) => (false, false),
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    use winreg::RegKey;
    use winreg::RegValue;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE, RegType};

    /// Folder menu keys: (key under Software\Classes, label, argument).
    const MENU_KEYS: [(&str, &str, &str); 3] = [
        (r"Directory\shell\MarkDraft", "Open with MarkDraft", "%1"),
        (r"Drive\shell\MarkDraft", "Open with MarkDraft", "%1"),
        (
            r"Directory\Background\shell\MarkDraft",
            "Open MarkDraft here",
            "%V",
        ),
    ];

    /// The machine-wide environment the MSI installer writes to.
    const MACHINE_ENVIRONMENT: &str =
        r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment";

    const MARKER: &str = "Created by MarkDraft";

    /// The launcher the installers ship next to the app (`bin\markdraft.cmd`).
    /// It finds the app relative to itself, so it never goes stale.
    fn bundled_bin(exe: &Path) -> Option<PathBuf> {
        let dir = exe.parent()?.join("bin");
        dir.join("markdraft.cmd").is_file().then_some(dir)
    }

    /// Where a portable or development copy writes its own launcher.
    fn own_bin() -> io::Result<PathBuf> {
        let base = std::env::var_os("LOCALAPPDATA")
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "LOCALAPPDATA is not set"))?;
        Ok(PathBuf::from(base).join("MarkDraft").join("bin"))
    }

    fn bin_dir(exe: &Path) -> io::Result<PathBuf> {
        bundled_bin(exe).map_or_else(own_bin, Ok)
    }

    /// `start` returns at once for the GUI; `export` and `--help` wait so
    /// their output and exit code reach the terminal. No arguments opens
    /// the current folder (`.`, which the app resolves against its cwd).
    pub(super) fn shim(exe: &Path) -> String {
        let exe = exe.display();
        format!(
            "@echo off\r\n\
             rem {MARKER}. Turn it off in Settings > System.\r\n\
             if \"%~1\"==\"\" (start \"\" \"{exe}\" . & exit /b 0)\r\n\
             for %%a in (export help -h --help -V --version) do if /i \"%~1\"==\"%%a\" (\"{exe}\" %* & exit /b)\r\n\
             start \"\" \"{exe}\" %*\r\n"
        )
    }

    /// A PATH value from the registry, without expanding `%VARS%`, and its type.
    fn read_path(env: &RegKey) -> (String, RegType) {
        match env.get_raw_value("Path") {
            Ok(v) if matches!(v.vtype, RegType::REG_SZ | RegType::REG_EXPAND_SZ) => {
                let wide: Vec<u16> = v
                    .bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| u16::from_le_bytes(*c))
                    .collect();
                let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
                (
                    OsString::from_wide(&wide[..end])
                        .to_string_lossy()
                        .into_owned(),
                    v.vtype,
                )
            }
            _ => (String::new(), RegType::REG_EXPAND_SZ),
        }
    }

    fn same_dir(entry: &str, dir: &Path) -> bool {
        let norm = |s: &str| s.trim().trim_end_matches('\\').to_lowercase();
        norm(entry) == norm(&dir.to_string_lossy())
    }

    fn path_has(root: winreg::HKEY, key: &str, dir: &Path) -> bool {
        RegKey::predef(root)
            .open_subkey_with_flags(key, KEY_READ)
            .map(|env| read_path(&env).0.split(';').any(|e| same_dir(e, dir)))
            .unwrap_or(false)
    }

    fn set_user_path(dir: &Path, present: bool) -> io::Result<()> {
        let env = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)?;
        let (path, vtype) = read_path(&env);
        let mut entries: Vec<&str> = path.split(';').filter(|e| !e.trim().is_empty()).collect();
        let had = entries.iter().any(|e| same_dir(e, dir));
        if had == present {
            return Ok(());
        }
        entries.retain(|e| !same_dir(e, dir));
        let dir = dir.to_string_lossy();
        if present {
            entries.push(&dir);
        }
        let mut bytes: Vec<u8> = Vec::new();
        for unit in std::ffi::OsStr::new(&entries.join(";"))
            .encode_wide()
            .chain([0])
        {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        env.set_raw_value(
            "Path",
            &RegValue {
                bytes: bytes.into(),
                vtype,
            },
        )?;
        crate::win32::broadcast_environment_change();
        Ok(())
    }

    pub fn install_cli(exe: &Path) -> io::Result<()> {
        let dir = bin_dir(exe)?;
        if bundled_bin(exe).is_none() {
            fs::create_dir_all(&dir)?;
            fs::write(dir.join("markdraft.cmd"), shim(exe))?;
        }
        if path_has(HKEY_LOCAL_MACHINE, MACHINE_ENVIRONMENT, &dir) {
            return Ok(());
        }
        set_user_path(&dir, true)
    }

    pub fn remove_cli(exe: &Path) -> io::Result<()> {
        if let Some(dir) = bundled_bin(exe) {
            set_user_path(&dir, false)?;
        }
        let own = own_bin()?;
        set_user_path(&own, false)?;
        // Only delete a launcher we generated, never the installer's.
        let shim = own.join("markdraft.cmd");
        if fs::read_to_string(&shim).is_ok_and(|t| t.contains(MARKER)) {
            remove_file(&shim)?;
            // Leave the folder if something else lives there.
            let _ = fs::remove_dir(own);
        }
        Ok(())
    }

    pub fn cli_status(exe: &Path) -> Feature {
        let Ok(dir) = bin_dir(exe) else {
            return unsupported("LOCALAPPDATA is not set");
        };
        let shim = dir.join("markdraft.cmd");
        let bundled = bundled_bin(exe).is_some();
        let (exists, stale) = file_state(&shim, exe);
        let machine = path_has(HKEY_LOCAL_MACHINE, MACHINE_ENVIRONMENT, &dir);
        let enabled = exists && (machine || path_has(HKEY_CURRENT_USER, "Environment", &dir));
        Feature {
            supported: true,
            enabled,
            stale: enabled && stale && !bundled,
            managed: enabled && machine,
            detail: if enabled && machine {
                "Added for all users by the MSI installer.".into()
            } else if enabled {
                format!("{} (open a new terminal to use it)", shim.display())
            } else {
                "Adds a markdraft command to your user PATH.".into()
            },
        }
    }

    fn classes() -> io::Result<RegKey> {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(r"Software\Classes", KEY_READ | KEY_WRITE)
    }

    pub fn install_menu(exe: &Path) -> io::Result<()> {
        let classes = classes()?;
        let exe = exe.display().to_string();
        for (key, label, arg) in MENU_KEYS {
            let (verb, _) = classes.create_subkey(key)?;
            verb.set_value("", &label)?;
            verb.set_value("Icon", &format!("\"{exe}\",0"))?;
            let (command, _) = verb.create_subkey("command")?;
            command.set_value("", &format!("\"{exe}\" \"{arg}\""))?;
        }
        Ok(())
    }

    pub fn remove_menu() -> io::Result<()> {
        let classes = classes()?;
        for (key, _, _) in MENU_KEYS {
            match classes.delete_subkey_all(key) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        Ok(())
    }

    fn menu_command(root: winreg::HKEY) -> Option<String> {
        RegKey::predef(root)
            .open_subkey(format!(r"Software\Classes\{}\command", MENU_KEYS[0].0))
            .and_then(|k| k.get_value::<String, _>(""))
            .ok()
    }

    pub fn menu_status(exe: &Path) -> Feature {
        let user = menu_command(HKEY_CURRENT_USER);
        let machine = if user.is_none() {
            menu_command(HKEY_LOCAL_MACHINE)
        } else {
            None
        };
        let managed = machine.is_some();
        let command = user.or(machine);
        let where_ = if managed {
            "Added for all users by the MSI installer."
        } else {
            "Right-click a folder, or empty space inside one."
        };
        Feature {
            supported: true,
            enabled: command.is_some(),
            stale: command.is_some_and(|c| !c.contains(&*exe.to_string_lossy())),
            managed,
            detail: format!("{where_} On Windows 11 the entries are under \"Show more options\"."),
        }
    }

    fn unsupported(why: &str) -> Feature {
        Feature {
            supported: false,
            enabled: false,
            stale: false,
            managed: false,
            detail: why.into(),
        }
    }
}

#[cfg(unix)]
mod platform {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    const MARKER: &str = "Created by MarkDraft";

    fn home() -> io::Result<PathBuf> {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))
    }

    /// Quotes for a POSIX shell.
    fn sh_quote(s: &str) -> String {
        format!("'{}'", s.replace('\'', r"'\''"))
    }

    fn write_executable(path: &Path, text: &str) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, text)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
    }

    /// Where the command can go: `/usr/local/bin` on macOS when writable
    /// (it is on the default PATH), otherwise `~/.local/bin`.
    fn cli_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        if cfg!(target_os = "macos") {
            dirs.push(PathBuf::from("/usr/local/bin"));
        }
        if let Ok(home) = home() {
            dirs.push(home.join(".local/bin"));
        }
        dirs
    }

    /// The command we installed, if any (only files carrying our marker).
    fn installed_cli() -> Option<PathBuf> {
        cli_dirs()
            .into_iter()
            .map(|d| d.join("markdraft"))
            .find(|p| fs::read_to_string(p).is_ok_and(|t| t.contains(MARKER)))
    }

    pub(super) fn cli_script(exe: &Path) -> String {
        format!(
            "#!/bin/sh\n\
             # {MARKER}. Turn it off in Settings > System.\n\
             APP={}\n\
             case \"$1\" in\n  export|help|-h|--help|-V|--version) exec \"$APP\" \"$@\" ;;\nesac\n\
             [ $# -eq 0 ] && set -- .\n\
             nohup \"$APP\" \"$@\" >/dev/null 2>&1 &\n",
            sh_quote(&exe.to_string_lossy())
        )
    }

    pub fn install_cli(exe: &Path) -> io::Result<()> {
        let script = cli_script(exe);
        let mut last = io::Error::new(io::ErrorKind::NotFound, "no folder for the command");
        for dir in cli_dirs() {
            let path = dir.join("markdraft");
            // Never replace a `markdraft` someone else put there.
            if path.exists() && !fs::read_to_string(&path).is_ok_and(|t| t.contains(MARKER)) {
                continue;
            }
            match write_executable(&path, &script) {
                Ok(()) => return Ok(()),
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    pub fn remove_cli(_exe: &Path) -> io::Result<()> {
        while let Some(path) = installed_cli() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    fn on_path(dir: &Path) -> bool {
        std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d == dir))
    }

    pub fn cli_status(exe: &Path) -> Feature {
        let Some(path) = installed_cli() else {
            return Feature {
                supported: true,
                enabled: false,
                stale: false,
                managed: false,
                detail: "Adds a markdraft command for your terminal.".into(),
            };
        };
        let (_, stale) = file_state(&path, exe);
        let dir = path.parent().unwrap_or(Path::new("/"));
        // GUI apps on macOS get a minimal PATH, so only Linux can tell.
        let hint = if cfg!(target_os = "linux") && !on_path(dir) {
            format!(" — add {} to your PATH", dir.display())
        } else if dir.ends_with(".local/bin") && cfg!(target_os = "macos") {
            " — make sure ~/.local/bin is on your PATH".to_owned()
        } else {
            String::new()
        };
        Feature {
            supported: true,
            enabled: true,
            stale,
            managed: false,
            detail: format!("{}{hint}", path.display()),
        }
    }

    /// File-manager entries: Nautilus (GNOME), Dolphin (KDE), Nemo (Cinnamon).
    fn menu_files() -> io::Result<[PathBuf; 3]> {
        let data = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .map_or_else(|| home().map(|h| h.join(".local/share")), Ok)?;
        Ok([
            data.join("nautilus/scripts/Open with MarkDraft"),
            data.join("kio/servicemenus/markdraft.desktop"),
            data.join("nemo/actions/markdraft.nemo_action"),
        ])
    }

    /// Quotes for an `Exec=` line in a desktop entry.
    fn exec_quote(s: &str) -> String {
        let mut out = String::from('"');
        for c in s.chars() {
            if matches!(c, '"' | '`' | '$' | '\\') {
                out.push('\\');
            }
            out.push(c);
        }
        out.push('"');
        out
    }

    pub fn install_menu(exe: &Path) -> io::Result<()> {
        if cfg!(target_os = "macos") {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "not available on macOS",
            ));
        }
        let exe = exe.to_string_lossy();
        let [nautilus, dolphin, nemo] = menu_files()?;
        // Nautilus runs scripts in the current folder with the selection as arguments.
        write_executable(
            &nautilus,
            &format!(
                "#!/bin/sh\n# {MARKER}.\nif [ $# -gt 0 ]; then exec {q} \"$1\"; fi\nexec {q} \"$(pwd)\"\n",
                q = sh_quote(&exe)
            ),
        )?;
        write_executable(
            &dolphin,
            &format!(
                "# {MARKER}.\n[Desktop Entry]\nType=Service\nMimeType=inode/directory;\nActions=openMarkDraft\n\
                 X-KDE-Priority=TopLevel\n\n[Desktop Action openMarkDraft]\nName=Open with MarkDraft\n\
                 Icon=markdraft\nExec={} %f\n",
                exec_quote(&exe)
            ),
        )?;
        if let Some(parent) = nemo.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(
            &nemo,
            format!(
                "# {MARKER}.\n[Nemo Action]\nName=Open with MarkDraft\nComment=Open this folder in MarkDraft\n\
                 Exec={} %F\nIcon-Name=markdraft\nSelection=s\nExtensions=dir;\n",
                exec_quote(&exe)
            ),
        )
    }

    pub fn remove_menu() -> io::Result<()> {
        if cfg!(target_os = "macos") {
            return Ok(());
        }
        for path in menu_files()? {
            if fs::read_to_string(&path).is_ok_and(|t| t.contains(MARKER)) {
                remove_file(&path)?;
            }
        }
        Ok(())
    }

    pub fn menu_status(exe: &Path) -> Feature {
        if cfg!(target_os = "macos") {
            return Feature {
                supported: false,
                enabled: false,
                stale: false,
                managed: false,
                detail:
                    "Finder has no per-app folder menu. Drag a folder onto MarkDraft in the Dock, \
                         or run markdraft . in Terminal."
                        .into(),
            };
        }
        let Ok([nautilus, ..]) = menu_files() else {
            return Feature {
                supported: false,
                enabled: false,
                stale: false,
                managed: false,
                detail: "HOME is not set".into(),
            };
        };
        let (enabled, stale) = file_state(&nautilus, exe);
        Feature {
            supported: true,
            enabled,
            stale,
            managed: false,
            detail: "GNOME Files: right-click → Scripts → Open with MarkDraft. Dolphin and Nemo: \
                     right-click a folder → Open with MarkDraft."
                .into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_state_detects_other_copies() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("shim");
        assert_eq!(file_state(&file, Path::new("/a/markdraft")), (false, false));
        fs::write(&file, "exec /a/markdraft").unwrap();
        assert_eq!(file_state(&file, Path::new("/a/markdraft")), (true, false));
        assert_eq!(file_state(&file, Path::new("/b/markdraft")), (true, true));
    }

    #[cfg(windows)]
    #[test]
    fn shim_opens_current_folder_and_waits_for_export() {
        let text = platform::shim(Path::new(r"C:\Apps\MarkDraft\markdraft.exe"));
        assert!(text.contains(r#"start "" "C:\Apps\MarkDraft\markdraft.exe" ."#));
        assert!(text.contains(r#"("C:\Apps\MarkDraft\markdraft.exe" %* & exit /b)"#));
    }

    #[cfg(unix)]
    #[test]
    fn script_quotes_the_app_path() {
        let text = platform::cli_script(Path::new("/opt/it's here/markdraft"));
        assert!(text.contains(r"APP='/opt/it'\''s here/markdraft'"));
        assert!(text.contains("set -- ."));
    }
}
