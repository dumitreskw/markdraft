//! Reading and saving documents without changing their on-disk format.
//!
//! Text is handed to the editor with `\n` line endings and no BOM; saving
//! restores the original line endings, BOM and encoding, and writes
//! atomically (temp file in the same folder, then rename).

use std::fs;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::Error;
use crate::scan::system_time_ms;

/// Documents larger than this are refused rather than freezing the editor.
pub const MAX_DOCUMENT_MB: u64 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum LineEnding {
    #[default]
    Lf,
    Crlf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TextEncoding {
    #[default]
    Utf8,
    Utf16Le,
    Utf16Be,
    /// Legacy 8-bit text that was not valid UTF-8.
    Windows1252,
}

/// How a document is stored on disk, so a save can reproduce it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DocFormat {
    pub line_ending: LineEnding,
    pub bom: bool,
    pub encoding: TextEncoding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedDoc {
    /// Contents with `\n` line endings and no BOM.
    pub text: String,
    pub format: DocFormat,
    pub modified_ms: i64,
    pub readonly: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum SaveOutcome {
    Saved {
        #[serde(rename = "modifiedMs")]
        modified_ms: i64,
    },
    /// The file changed on disk since it was loaded; nothing was written.
    Conflict {
        #[serde(rename = "diskModifiedMs")]
        disk_modified_ms: i64,
    },
}

pub fn read_document(path: &Path) -> Result<LoadedDoc, Error> {
    let meta = fs::metadata(path).map_err(|e| Error::io(path, e))?;
    if meta.len() > MAX_DOCUMENT_MB * 1024 * 1024 {
        return Err(Error::TooLarge(path.to_owned(), MAX_DOCUMENT_MB));
    }
    let bytes = fs::read(path).map_err(|e| Error::io(path, e))?;
    let (raw, encoding, bom) = decode(&bytes).ok_or_else(|| Error::Binary(path.to_owned()))?;

    let crlf = raw.matches("\r\n").count();
    let lf_only = raw.matches('\n').count() - crlf;
    let line_ending = if crlf > lf_only {
        LineEnding::Crlf
    } else {
        LineEnding::Lf
    };
    Ok(LoadedDoc {
        text: raw.replace("\r\n", "\n"),
        format: DocFormat {
            line_ending,
            bom,
            encoding,
        },
        modified_ms: meta.modified().map(system_time_ms).unwrap_or(0),
        readonly: meta.permissions().readonly(),
    })
}

fn decode(bytes: &[u8]) -> Option<(String, TextEncoding, bool)> {
    if let Some(rest) = bytes.strip_prefix(b"\xEF\xBB\xBF") {
        return Some((
            String::from_utf8_lossy(rest).into_owned(),
            TextEncoding::Utf8,
            true,
        ));
    }
    if let Some(rest) = bytes.strip_prefix(b"\xFF\xFE") {
        let (text, _) = encoding_rs::UTF_16LE.decode_without_bom_handling(rest);
        return Some((text.into_owned(), TextEncoding::Utf16Le, true));
    }
    if let Some(rest) = bytes.strip_prefix(b"\xFE\xFF") {
        let (text, _) = encoding_rs::UTF_16BE.decode_without_bom_handling(rest);
        return Some((text.into_owned(), TextEncoding::Utf16Be, true));
    }
    let probe = &bytes[..bytes.len().min(8192)];
    if probe.contains(&0) {
        return None;
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => Some((text.to_owned(), TextEncoding::Utf8, false)),
        Err(_) => {
            let (text, _) = encoding_rs::WINDOWS_1252.decode_without_bom_handling(bytes);
            Some((text.into_owned(), TextEncoding::Windows1252, false))
        }
    }
}

fn encode(text: &str, format: &DocFormat) -> Vec<u8> {
    let text = match format.line_ending {
        LineEnding::Lf => std::borrow::Cow::Borrowed(text),
        LineEnding::Crlf => std::borrow::Cow::Owned(text.replace('\n', "\r\n")),
    };
    let mut out = Vec::with_capacity(text.len() + 3);
    match format.encoding {
        TextEncoding::Utf8 => {
            if format.bom {
                out.extend_from_slice(b"\xEF\xBB\xBF");
            }
            out.extend_from_slice(text.as_bytes());
        }
        TextEncoding::Utf16Le => {
            out.extend_from_slice(b"\xFF\xFE");
            out.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        }
        TextEncoding::Utf16Be => {
            out.extend_from_slice(b"\xFE\xFF");
            out.extend(text.encode_utf16().flat_map(u16::to_be_bytes));
        }
        TextEncoding::Windows1252 => {
            let (bytes, _, _) = encoding_rs::WINDOWS_1252.encode(&text);
            out.extend_from_slice(&bytes);
        }
    }
    out
}

/// Saves `text` (with `\n` line endings) in `format`. When
/// `expected_modified_ms` is given and the file on disk has a different
/// modification time, nothing is written and `Conflict` is returned.
pub fn save_document(
    path: &Path,
    text: &str,
    format: &DocFormat,
    expected_modified_ms: Option<i64>,
) -> Result<SaveOutcome, Error> {
    if let (Some(expected), Ok(meta)) = (expected_modified_ms, fs::metadata(path)) {
        let disk = meta.modified().map(system_time_ms).unwrap_or(0);
        if disk != expected {
            return Ok(SaveOutcome::Conflict {
                disk_modified_ms: disk,
            });
        }
    }
    write_atomic(path, &encode(text, format)).map_err(|e| Error::io(path, e))?;
    let modified_ms = fs::metadata(path)
        .and_then(|m| m.modified())
        .map(system_time_ms)
        .map_err(|e| Error::io(path, e))?;
    Ok(SaveOutcome::Saved { modified_ms })
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| std::io::Error::other("path has no file name"))?;
    if fs::metadata(path).is_ok_and(|m| m.permissions().readonly()) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "the file is read-only",
        ));
    }
    let tmp = dir.join(format!(".{}.markdraft-tmp", name.to_string_lossy()));
    let result = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(bytes: &[u8]) -> (LoadedDoc, Vec<u8>) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.md");
        fs::write(&path, bytes).unwrap();
        let doc = read_document(&path).unwrap();
        let outcome = save_document(&path, &doc.text, &doc.format, Some(doc.modified_ms)).unwrap();
        assert!(matches!(outcome, SaveOutcome::Saved { .. }));
        (doc, fs::read(&path).unwrap())
    }

    #[test]
    fn crlf_and_bom_survive_a_round_trip() {
        let original = b"\xEF\xBB\xBF# T\r\n\r\nbody\r\n";
        let (doc, saved) = round_trip(original);
        assert_eq!(doc.text, "# T\n\nbody\n");
        assert_eq!(
            doc.format,
            DocFormat {
                line_ending: LineEnding::Crlf,
                bom: true,
                encoding: TextEncoding::Utf8
            }
        );
        assert_eq!(saved, original);
    }

    #[test]
    fn utf16_and_legacy_encodings_survive() {
        let mut utf16 = vec![0xFF, 0xFE];
        utf16.extend("héllo\n".encode_utf16().flat_map(u16::to_le_bytes));
        let (doc, saved) = round_trip(&utf16);
        assert_eq!(doc.text, "héllo\n");
        assert_eq!(doc.format.encoding, TextEncoding::Utf16Le);
        assert_eq!(saved, utf16);

        let latin = b"caf\xE9\n";
        let (doc, saved) = round_trip(latin);
        assert_eq!(doc.text, "café\n");
        assert_eq!(doc.format.encoding, TextEncoding::Windows1252);
        assert_eq!(saved, latin);
    }

    #[test]
    fn binary_files_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.md");
        fs::write(&path, b"PK\x03\x04\x00\x00binary").unwrap();
        assert!(matches!(read_document(&path), Err(Error::Binary(_))));
    }

    #[test]
    fn stale_saves_report_a_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.md");
        fs::write(&path, "v1").unwrap();
        let doc = read_document(&path).unwrap();
        let outcome =
            save_document(&path, "mine", &doc.format, Some(doc.modified_ms - 5000)).unwrap();
        assert!(matches!(outcome, SaveOutcome::Conflict { .. }));
        assert_eq!(fs::read_to_string(&path).unwrap(), "v1");
        // No temp files left behind.
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn new_files_can_be_saved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.md");
        let outcome = save_document(&path, "hi\n", &DocFormat::default(), None).unwrap();
        assert!(matches!(outcome, SaveOutcome::Saved { .. }));
        assert_eq!(fs::read_to_string(&path).unwrap(), "hi\n");
    }

    #[test]
    fn save_outcome_json_shape() {
        let json = serde_json::to_string(&SaveOutcome::Saved { modified_ms: 5 }).unwrap();
        assert_eq!(json, r#"{"status":"saved","modifiedMs":5}"#);
    }
}
