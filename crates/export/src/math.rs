//! LaTeX math → Typst math via `mitex`.
//!
//! `mitex` converts LaTeX into Typst math code that refers to helper
//! definitions from the `@preview/mitex` Typst package (`mitexsqrt`,
//! `mitexdisplay`, `pmatrix`, ...). Those definitions are vendored under
//! `typst/mitex/` and served from memory by the export world, so no package
//! is ever downloaded.

use typst::syntax::parse_math;

/// Converts a LaTeX math snippet to Typst math code.
///
/// Returns `None` when mitex cannot convert it or the result does not parse;
/// the caller then shows the LaTeX source verbatim instead.
pub fn latex_to_typst(latex: &str) -> Option<String> {
    let trimmed = latex.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Defence in depth against converter panics on malformed input. This
    // only helps where panics unwind (tests, debug builds); the app's release
    // profile aborts, so `malformed_input_does_not_panic` covers known cases.
    let converted = std::panic::catch_unwind(|| mitex::convert_math(trimmed, None))
        .ok()?
        .ok()?;
    if !parse_math(&converted).errors_and_warnings().0.is_empty() {
        return None;
    }
    Some(converted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_common_latex() {
        assert_eq!(
            latex_to_typst(r"\frac{a}{b}").as_deref(),
            Some("frac(a ,b )")
        );
        assert!(latex_to_typst(r"\sum_{i=1}^n i^2").is_some());
        assert!(latex_to_typst(r"\begin{pmatrix} 1 & 0 \\ 0 & 1 \end{pmatrix}").is_some());
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(latex_to_typst("  "), None);
    }

    #[test]
    fn malformed_input_does_not_panic() {
        for latex in [
            r"\frac{",
            r"}",
            r"\begin{pmatrix",
            r"\end{cases}",
            r"\left(",
            r"\right)",
            r"x^",
            r"_",
            r"\sqrt[",
            r"&&&",
            r"\\\\",
            r"\",
            r"{{{{",
            r"\unknowncommand{x}",
            r"\begin{array}{c|c} a \end{array}",
            r"\text{",
            r"\color{red",
            r"$",
            r"%comment",
            r"\over\over",
            r"\mathbb",
            r"\operatorname*{arg\,max}_x",
        ] {
            let result = std::panic::catch_unwind(|| mitex::convert_math(latex, None));
            assert!(result.is_ok(), "mitex panicked on {latex:?}");
            let _ = latex_to_typst(latex);
        }
    }
}
