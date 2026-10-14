//! The bundled fonts are the only fonts the exporter can use.

#[test]
fn bundled_font_families() {
    let families = export::bundled_font_families();
    assert_eq!(
        families,
        [
            "Inter",
            "JetBrains Mono NL",
            "New Computer Modern Math",
            "Noto Emoji",
            "Source Serif 4",
        ]
    );
}
