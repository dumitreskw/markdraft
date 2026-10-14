//! Command-line interface shared by the MarkDraft app and `markdraft-cli`.
//!
//! `markdraft <path>` opens a folder, file or workspace in the app;
//! `markdraft export <file> -o out.pdf` runs headlessly without a window.

use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::{Parser, Subcommand};
use export::{ExportOptions, Paper, Theme};

#[derive(Debug, Parser, PartialEq)]
#[command(name = "markdraft", version, about = "Read, write and export Markdown")]
pub struct Cli {
    /// Folder, Markdown file or .markdraft-workspace to open.
    pub path: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand, PartialEq)]
pub enum Command {
    /// Export a Markdown file to PDF without opening a window.
    Export {
        /// Markdown file to export.
        input: PathBuf,
        /// Output PDF path; defaults to the input with a .pdf extension.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Theme: clean, academic or compact.
        #[arg(long, default_value = "clean")]
        theme: Theme,
        /// Paper size: A4 or Letter.
        #[arg(long, default_value = "A4")]
        paper: Paper,
        /// Include a table of contents.
        #[arg(long)]
        toc: bool,
    },
}

/// Runs a headless subcommand. Returns `Ok(false)` when there is none and
/// the caller should start the UI instead.
pub fn run_headless(cli: &Cli) -> anyhow::Result<bool> {
    let Some(Command::Export {
        input,
        output,
        theme,
        paper,
        toc,
    }) = &cli.command
    else {
        return Ok(false);
    };
    let output = output.clone().unwrap_or_else(|| default_output(input));
    let markdown =
        std::fs::read_to_string(input).with_context(|| format!("reading {}", input.display()))?;
    let opts = ExportOptions {
        theme: *theme,
        paper: *paper,
        toc: *toc,
    };
    let pdf = export::markdown_to_pdf(&markdown, &opts)?;
    std::fs::write(&output, pdf).with_context(|| format!("writing {}", output.display()))?;
    Ok(true)
}

fn default_output(input: &Path) -> PathBuf {
    input.with_extension("pdf")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_path_opens() {
        let cli = Cli::try_parse_from(["markdraft", "docs"]).unwrap();
        assert_eq!(cli.path, Some(PathBuf::from("docs")));
        assert_eq!(cli.command, None);
    }

    #[test]
    fn export_with_options() {
        let cli = Cli::try_parse_from([
            "markdraft",
            "export",
            "a.md",
            "-o",
            "b.pdf",
            "--theme",
            "academic",
            "--paper",
            "letter",
            "--toc",
        ])
        .unwrap();
        assert_eq!(
            cli.command,
            Some(Command::Export {
                input: "a.md".into(),
                output: Some("b.pdf".into()),
                theme: Theme::Academic,
                paper: Paper::Letter,
                toc: true,
            })
        );
    }

    #[test]
    fn rejects_unknown_theme() {
        assert!(Cli::try_parse_from(["markdraft", "export", "a.md", "--theme", "x"]).is_err());
    }

    #[test]
    fn default_output_swaps_extension() {
        assert_eq!(
            default_output(Path::new("dir/a.md")),
            PathBuf::from("dir/a.pdf")
        );
    }
}
