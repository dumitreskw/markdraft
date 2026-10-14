//! PDF and HTML export.
//!
//! M0 defines the export options shared by the app, the CLI and workspace
//! files. The Markdown → Typst → PDF pipeline lands in M3.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unknown theme `{0}` (expected clean, academic or compact)")]
    UnknownTheme(String),
    #[error("unknown paper size `{0}` (expected A4 or Letter)")]
    UnknownPaper(String),
    #[error("PDF export is not implemented yet (planned for milestone M3)")]
    NotImplemented,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Clean,
    Academic,
    Compact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Paper {
    #[default]
    A4,
    Letter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportOptions {
    pub theme: Theme,
    pub paper: Paper,
    pub toc: bool,
}

impl FromStr for Theme {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Error> {
        match s.to_ascii_lowercase().as_str() {
            "clean" => Ok(Self::Clean),
            "academic" => Ok(Self::Academic),
            "compact" => Ok(Self::Compact),
            _ => Err(Error::UnknownTheme(s.to_owned())),
        }
    }
}

impl fmt::Display for Theme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Clean => "clean",
            Self::Academic => "academic",
            Self::Compact => "compact",
        })
    }
}

impl FromStr for Paper {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Error> {
        match s.to_ascii_lowercase().as_str() {
            "a4" => Ok(Self::A4),
            "letter" => Ok(Self::Letter),
            _ => Err(Error::UnknownPaper(s.to_owned())),
        }
    }
}

impl fmt::Display for Paper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::A4 => "A4",
            Self::Letter => "Letter",
        })
    }
}

/// Exports one Markdown document to PDF bytes.
pub fn markdown_to_pdf(_markdown: &str, _opts: &ExportOptions) -> Result<Vec<u8>, Error> {
    Err(Error::NotImplemented)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_names_case_insensitively() {
        assert_eq!("Academic".parse::<Theme>().unwrap(), Theme::Academic);
        assert_eq!("letter".parse::<Paper>().unwrap(), Paper::Letter);
        assert!("fancy".parse::<Theme>().is_err());
        assert!("A5".parse::<Paper>().is_err());
    }

    #[test]
    fn display_round_trips() {
        for t in [Theme::Clean, Theme::Academic, Theme::Compact] {
            assert_eq!(t.to_string().parse::<Theme>().unwrap(), t);
        }
        for p in [Paper::A4, Paper::Letter] {
            assert_eq!(p.to_string().parse::<Paper>().unwrap(), p);
        }
    }

    #[test]
    fn options_json_shape() {
        let json = serde_json::to_string(&ExportOptions::default()).unwrap();
        assert_eq!(json, r#"{"theme":"clean","paper":"A4","toc":false}"#);
    }
}
