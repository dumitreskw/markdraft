//! Per-user app data: settings, recent items and session state.
//!
//! Settings and session state are JSON documents whose shape the UI owns;
//! this module only stores them. Everything is written atomically.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::write_atomic;

/// Maximum number of recent items kept.
pub const MAX_RECENTS: usize = 20;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("no per-user data directory is available on this system")]
    NoHome,
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecentKind {
    Folder,
    File,
    Workspace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentEntry {
    pub path: PathBuf,
    pub kind: RecentKind,
    pub name: String,
    pub opened_ms: i64,
}

/// The app's per-user data folder (for example `%APPDATA%\markdraft\MarkDraft\data`).
#[derive(Debug, Clone)]
pub struct AppStore {
    dir: PathBuf,
}

impl AppStore {
    pub fn open_default() -> Result<Self, StoreError> {
        let dirs = directories::ProjectDirs::from("app", "markdraft", "MarkDraft")
            .ok_or(StoreError::NoHome)?;
        Ok(Self::at(dirs.data_dir().to_path_buf()))
    }

    pub fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    fn read_json<T: for<'de> Deserialize<'de>>(&self, name: &str) -> Option<T> {
        let text = fs::read_to_string(self.path(name)).ok()?;
        // A corrupt file must never stop the app from starting.
        serde_json::from_str(&text).ok()
    }

    fn write_json<T: Serialize>(&self, name: &str, value: &T) -> Result<(), StoreError> {
        let path = self.path(name);
        let io = |source| StoreError::Io {
            path: path.clone(),
            source,
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(io)?;
        }
        let text = serde_json::to_string_pretty(value).expect("JSON values always serialise");
        write_atomic(&path, text.as_bytes()).map_err(io)
    }

    pub fn settings(&self) -> Value {
        self.read_json("settings.json")
            .unwrap_or(Value::Object(Default::default()))
    }

    pub fn save_settings(&self, settings: &Value) -> Result<(), StoreError> {
        self.write_json("settings.json", settings)
    }

    /// Recent items whose paths still exist, newest first.
    pub fn recents(&self) -> Vec<RecentEntry> {
        let mut items: Vec<RecentEntry> = self.read_json("recents.json").unwrap_or_default();
        items.retain(|r| r.path.exists());
        items
    }

    /// Moves `entry` to the top of the recent list.
    pub fn push_recent(&self, entry: RecentEntry) -> Result<Vec<RecentEntry>, StoreError> {
        let mut items: Vec<RecentEntry> = self.read_json("recents.json").unwrap_or_default();
        items.retain(|r| r.path != entry.path);
        items.insert(0, entry);
        items.truncate(MAX_RECENTS);
        self.write_json("recents.json", &items)?;
        Ok(items)
    }

    pub fn remove_recent(&self, path: &Path) -> Result<Vec<RecentEntry>, StoreError> {
        let mut items: Vec<RecentEntry> = self.read_json("recents.json").unwrap_or_default();
        items.retain(|r| r.path != path);
        self.write_json("recents.json", &items)?;
        Ok(items)
    }

    /// UI session state (open tabs, layout) for a set of roots.
    pub fn session(&self, key: &str) -> Option<Value> {
        self.read_json(&session_file(key))
    }

    pub fn save_session(&self, key: &str, session: &Value) -> Result<(), StoreError> {
        self.write_json(&session_file(key), session)
    }
}

fn session_file(key: &str) -> String {
    format!("sessions/{key}.json")
}

/// A stable key for a set of root folders, independent of order and case.
pub fn session_key<P: AsRef<Path>>(roots: &[P]) -> String {
    let mut parts: Vec<String> = roots
        .iter()
        .map(|p| p.as_ref().to_string_lossy().to_lowercase())
        .collect();
    parts.sort();
    format!("{:016x}", fnv1a(parts.join("\n").as_bytes()))
}

/// FNV-1a: stable across Rust versions, unlike `DefaultHasher`.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn store() -> (tempfile::TempDir, AppStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = AppStore::at(dir.path().join("data"));
        (dir, store)
    }

    #[test]
    fn settings_round_trip_and_default_to_empty() {
        let (_d, s) = store();
        assert_eq!(s.settings(), json!({}));
        s.save_settings(&json!({"theme": "dark"})).unwrap();
        assert_eq!(s.settings(), json!({"theme": "dark"}));
    }

    #[test]
    fn corrupt_files_are_ignored() {
        let (_d, s) = store();
        fs::create_dir_all(s.dir()).unwrap();
        fs::write(s.dir().join("settings.json"), "{not json").unwrap();
        assert_eq!(s.settings(), json!({}));
    }

    #[test]
    fn recents_dedupe_order_and_cap() {
        let (d, s) = store();
        let entry = |i: i64| {
            let path = d.path().join(format!("f{i}"));
            fs::create_dir_all(&path).unwrap();
            RecentEntry {
                path,
                kind: RecentKind::Folder,
                name: format!("f{i}"),
                opened_ms: i,
            }
        };
        for i in 0..25 {
            s.push_recent(entry(i)).unwrap();
        }
        let items = s.push_recent(entry(3)).unwrap();
        assert_eq!(items.len(), MAX_RECENTS);
        assert_eq!(items[0].name, "f3");
        assert_eq!(items.iter().filter(|r| r.name == "f3").count(), 1);
        fs::remove_dir(d.path().join("f24")).unwrap();
        assert!(s.recents().iter().all(|r| r.name != "f24"));
    }

    #[test]
    fn sessions_are_keyed_by_root_set() {
        let (_d, s) = store();
        let key = session_key(&["/b", "/a"]);
        assert_eq!(key, session_key(&["/a", "/B"]));
        assert_ne!(key, session_key(&["/a"]));
        s.save_session(&key, &json!({"tabs": ["x.md"]})).unwrap();
        assert_eq!(s.session(&key), Some(json!({"tabs": ["x.md"]})));
    }
}
