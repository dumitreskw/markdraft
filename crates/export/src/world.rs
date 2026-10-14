//! In-memory Typst world: bundled fonts, bundled templates and the assets
//! referenced by one generated document. Nothing is read from disk or the
//! network during compilation, and system fonts are never consulted.

use std::collections::HashMap;
use std::sync::LazyLock;

use typst::diag::{FileError, FileResult, Severity, SourceDiagnostic};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World, WorldExt};
use typst_layout::PagedDocument;

use crate::Theme;

/// Fonts compiled into the binary. See `fonts/` for licenses.
static FONT_FILES: &[&[u8]] = &[
    include_bytes!("../fonts/Inter-Regular.ttf"),
    include_bytes!("../fonts/Inter-Italic.ttf"),
    include_bytes!("../fonts/Inter-SemiBold.ttf"),
    include_bytes!("../fonts/Inter-SemiBoldItalic.ttf"),
    include_bytes!("../fonts/SourceSerif4-Regular.otf"),
    include_bytes!("../fonts/SourceSerif4-It.otf"),
    include_bytes!("../fonts/SourceSerif4-Bold.otf"),
    include_bytes!("../fonts/SourceSerif4-BoldIt.otf"),
    include_bytes!("../fonts/JetBrainsMonoNL-Regular.ttf"),
    include_bytes!("../fonts/JetBrainsMonoNL-Italic.ttf"),
    include_bytes!("../fonts/JetBrainsMonoNL-Bold.ttf"),
    include_bytes!("../fonts/NewCMMath-Book.otf"),
];

/// Typst sources bundled with the exporter, by virtual path.
static TEMPLATES: &[(&str, &str)] = &[
    ("/markdraft/common.typ", include_str!("../typst/common.typ")),
    ("/markdraft/math.typ", include_str!("../typst/math.typ")),
    (
        "/markdraft/themes/clean.typ",
        include_str!("../typst/themes/clean.typ"),
    ),
    (
        "/markdraft/themes/academic.typ",
        include_str!("../typst/themes/academic.typ"),
    ),
    (
        "/markdraft/themes/compact.typ",
        include_str!("../typst/themes/compact.typ"),
    ),
    (
        "/markdraft/mitex/mod.typ",
        include_str!("../typst/mitex/mod.typ"),
    ),
    (
        "/markdraft/mitex/prelude.typ",
        include_str!("../typst/mitex/prelude.typ"),
    ),
    (
        "/markdraft/mitex/standard.typ",
        include_str!("../typst/mitex/standard.typ"),
    ),
];

/// Virtual path of a theme template, as imported by generated documents.
pub(crate) fn theme_path(theme: Theme) -> &'static str {
    match theme {
        Theme::Clean => "/markdraft/themes/clean.typ",
        Theme::Academic => "/markdraft/themes/academic.typ",
        Theme::Compact => "/markdraft/themes/compact.typ",
    }
}

struct Fonts {
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
}

static FONTS: LazyLock<Fonts> = LazyLock::new(|| {
    let fonts: Vec<Font> = FONT_FILES
        .iter()
        .flat_map(|data| Font::iter(Bytes::new(*data)))
        .collect();
    Fonts {
        book: LazyHash::new(FontBook::from_fonts(&fonts)),
        fonts,
    }
});

static LIBRARY: LazyLock<LazyHash<Library>> = LazyLock::new(|| LazyHash::new(Library::default()));

static TEMPLATE_SOURCES: LazyLock<HashMap<FileId, Source>> = LazyLock::new(|| {
    TEMPLATES
        .iter()
        .map(|(path, text)| {
            let id = file_id(path);
            (id, Source::new(id, (*text).to_owned()))
        })
        .collect()
});

fn file_id(path: &str) -> FileId {
    let vpath = VirtualPath::new(path).expect("bundled virtual paths are valid");
    RootedPath::new(VirtualRoot::Project, vpath).intern()
}

/// Family names of the bundled fonts (for tests and diagnostics).
pub(crate) fn font_families() -> Vec<String> {
    FONTS
        .book
        .families()
        .map(|(name, _)| name.to_owned())
        .collect()
}

/// A world holding one generated main file plus its assets.
pub(crate) struct ExportWorld {
    main: Source,
    assets: HashMap<FileId, Bytes>,
}

impl ExportWorld {
    pub fn new(main_text: String, assets: Vec<(String, Bytes)>) -> Self {
        let main = Source::new(file_id("/main.typ"), main_text);
        let assets = assets
            .into_iter()
            .map(|(path, data)| (file_id(&path), data))
            .collect();
        Self { main, assets }
    }

    /// Compiles the document, returning readable diagnostics on failure.
    pub fn compile(&self) -> Result<PagedDocument, String> {
        self.compile_with_warnings().0
    }

    /// Like [`Self::compile`], also returning Typst's warnings.
    pub fn compile_with_warnings(&self) -> (Result<PagedDocument, String>, Vec<String>) {
        let result = typst::compile::<PagedDocument>(self);
        // Keep the global memoization cache from growing without bound in a
        // long-running app; recent entries survive for quick re-exports.
        typst::comemo::evict(5);
        let warnings = result
            .warnings
            .iter()
            .map(|w| self.format_diagnostic(w))
            .collect();
        let output = result
            .output
            .map_err(|errors| self.format_diagnostics(&errors));
        (output, warnings)
    }

    fn format_diagnostics(&self, diags: &[SourceDiagnostic]) -> String {
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| self.format_diagnostic(d))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn format_diagnostic(&self, diag: &SourceDiagnostic) -> String {
        let mut msg = String::new();
        if let Some(location) = self.describe_span(diag) {
            msg.push_str(&location);
            msg.push_str(": ");
        }
        msg.push_str(&diag.message);
        for hint in &diag.hints {
            msg.push_str("\n  hint: ");
            msg.push_str(&hint.v);
        }
        msg
    }

    fn describe_span(&self, diag: &SourceDiagnostic) -> Option<String> {
        let id = diag.span.id()?;
        let range = self.range(diag.span)?;
        let source = self.source(id).ok()?;
        let line = source.lines().byte_to_line(range.start)?;
        let text = source.text().lines().nth(line).unwrap_or("").trim();
        let short: String = text.chars().take(80).collect();
        let path = id.vpath().get_with_slash();
        Some(format!("{path}:{}: `{short}`", line + 1))
    }
}

impl World for ExportWorld {
    fn library(&self) -> &LazyHash<Library> {
        &LIBRARY
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &FONTS.book
    }

    fn main(&self) -> FileId {
        self.main.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            return Ok(self.main.clone());
        }
        TEMPLATE_SOURCES
            .get(&id)
            .cloned()
            .ok_or_else(|| not_found(id))
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if let Some(data) = self.assets.get(&id) {
            return Ok(data.clone());
        }
        self.source(id)
            .map(|s| Bytes::from_string(s.text().to_owned()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        FONTS.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        // Exports must be reproducible, so there is no notion of "today".
        None
    }
}

fn not_found(id: FileId) -> FileError {
    FileError::NotFound(id.vpath().get_with_slash().into())
}
