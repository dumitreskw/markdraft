//! Workspace files (`*.markdraft-workspace`), settings and session state.
//!
//! A workspace file is plain TOML meant to be committed alongside the docs
//! it describes. Per-user session state (open tabs, pane sizes) is stored
//! separately in the OS app-data directory and arrives in M3.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// File extension of workspace files, without the leading dot.
pub const WORKSPACE_EXTENSION: &str = "markdraft-workspace";

/// Current on-disk format version.
pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not read workspace {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not write workspace {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid workspace {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: Box<toml::de::Error>,
    },
    #[error(
        "workspace {path} uses format version {found}; this build supports up to {FORMAT_VERSION}"
    )]
    UnsupportedVersion { path: PathBuf, found: u32 },
}

/// The contents of a `*.markdraft-workspace` file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceDef {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default, rename = "roots")]
    pub roots: Vec<Root>,
    #[serde(default)]
    pub discovery: Discovery,
    #[serde(default)]
    pub export: ExportSettings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Root {
    /// Absolute, or relative to the workspace file's folder.
    pub path: PathBuf,
    /// Display name; defaults to the folder name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Discovery {
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    pub respect_gitignore: bool,
}

impl Default for Discovery {
    fn default() -> Self {
        Self {
            include: vec!["**/*.md".into(), "**/*.markdown".into()],
            exclude: Vec::new(),
            respect_gitignore: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportSettings {
    pub theme: String,
    pub paper: String,
    pub toc: bool,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            theme: "clean".into(),
            paper: "A4".into(),
            toc: false,
        }
    }
}

fn default_version() -> u32 {
    FORMAT_VERSION
}

impl WorkspaceDef {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: FORMAT_VERSION,
            roots: Vec::new(),
            discovery: Discovery::default(),
            export: ExportSettings::default(),
        }
    }

    pub fn load(path: &Path) -> Result<Self, Error> {
        let text = fs::read_to_string(path).map_err(|source| Error::Read {
            path: path.to_owned(),
            source,
        })?;
        let def: Self = toml::from_str(&text).map_err(|source| Error::Parse {
            path: path.to_owned(),
            source: Box::new(source),
        })?;
        if def.version > FORMAT_VERSION {
            return Err(Error::UnsupportedVersion {
                path: path.to_owned(),
                found: def.version,
            });
        }
        Ok(def)
    }

    /// Writes the workspace atomically: a temp file in the same folder,
    /// then a rename over the target.
    pub fn save(&self, path: &Path) -> Result<(), Error> {
        let text = toml::to_string_pretty(self).expect("workspace is always serialisable");
        write_atomic(path, text.as_bytes()).map_err(|source| Error::Write {
            path: path.to_owned(),
            source,
        })
    }

    /// Resolves each root against the workspace file's folder.
    pub fn resolved_roots(&self, workspace_file: &Path) -> Vec<PathBuf> {
        let base = workspace_file.parent().unwrap_or(Path::new("."));
        self.roots
            .iter()
            .map(|r| {
                if r.path.is_absolute() {
                    r.path.clone()
                } else {
                    base.join(&r.path)
                }
            })
            .collect()
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let file_name = path
        .file_name()
        .ok_or_else(|| std::io::Error::other("path has no file name"))?;
    let tmp = dir.join(format!(".{}.tmp", file_name.to_string_lossy()));
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
name = "Platform docs"
version = 1

[[roots]]
path = "../repo/docs"

[[roots]]
path = "notes"
name = "Notes"

[discovery]
include = ["**/*.md"]
exclude = ["**/archive/**"]
respect_gitignore = true

[export]
theme = "academic"
paper = "Letter"
toc = true
"#;

    #[test]
    fn parses_documented_format() {
        let def: WorkspaceDef = toml::from_str(SAMPLE).unwrap();
        assert_eq!(def.name, "Platform docs");
        assert_eq!(def.roots.len(), 2);
        assert_eq!(def.roots[1].name.as_deref(), Some("Notes"));
        assert_eq!(def.discovery.exclude, vec!["**/archive/**"]);
        assert_eq!(def.export.theme, "academic");
    }

    #[test]
    fn minimal_file_uses_defaults() {
        let def: WorkspaceDef = toml::from_str("name = \"x\"").unwrap();
        assert_eq!(def, WorkspaceDef::new("x"));
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("team.markdraft-workspace");
        let mut def = WorkspaceDef::new("Team");
        def.roots.push(Root {
            path: "docs".into(),
            name: None,
        });
        def.save(&path).unwrap();
        assert_eq!(WorkspaceDef::load(&path).unwrap(), def);
        // No temp file left behind.
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn rejects_newer_versions_and_unknown_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("w.markdraft-workspace");
        fs::write(&path, "name = \"x\"\nversion = 99\n").unwrap();
        assert!(matches!(
            WorkspaceDef::load(&path),
            Err(Error::UnsupportedVersion { found: 99, .. })
        ));
        fs::write(&path, "name = \"x\"\ncolour = \"red\"\n").unwrap();
        assert!(matches!(
            WorkspaceDef::load(&path),
            Err(Error::Parse { .. })
        ));
    }

    #[test]
    fn relative_roots_resolve_against_workspace_folder() {
        let mut def = WorkspaceDef::new("x");
        def.roots.push(Root {
            path: "docs".into(),
            name: None,
        });
        let file = Path::new("/projects/team/w.markdraft-workspace");
        assert_eq!(
            def.resolved_roots(file),
            vec![PathBuf::from("/projects/team/docs")]
        );
    }
}
