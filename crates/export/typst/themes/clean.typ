// MarkDraft theme "clean": Inter, generous margins, a teal accent.
#import "/markdraft/common.typ" as common

#let theme = (
  ink: rgb("#1c2024"),
  muted: rgb("#5f6b76"),
  accent: rgb("#2f6f62"),
  link: rgb("#2f6f62"),
  rule: rgb("#d5dbe0"),
  code-bg: rgb("#f5f7f8"),
  code-border: rgb("#e3e8eb"),
)

#let md-alert = common.md-alert.with(theme: theme)
#let md-tasks = common.md-tasks.with(theme: theme)
#let md-image = common.md-image.with(theme: theme)
#let md-diagram = common.md-diagram.with(theme: theme)
#let md-missing-image = common.md-missing-image.with(theme: theme)
#let md-math-source = common.md-math-source.with(theme: theme)
#let md-rule = common.md-rule.with(theme: theme)

#let md-setup(paper: "a4", title: none, author: none, body) = {
  set document(title: title, author: if author != none { author } else { () }, date: none)
  set page(
    paper: paper,
    margin: (x: 2.4cm, top: 2.5cm, bottom: 2.7cm),
    footer: common.page-number(theme: theme),
    footer-descent: 40%,
  )
  set text(font: "Inter", size: 10.5pt, fill: theme.ink, lang: "en", hyphenate: false)
  set par(leading: 0.68em, spacing: 1.15em, justify: false)
  set list(indent: 0.3em, body-indent: 0.55em, spacing: auto)
  set enum(indent: 0.3em, body-indent: 0.55em)

  set heading(numbering: none)
  show heading: set text(weight: "semibold", fill: theme.ink)
  show heading: set block(above: 1.75em, below: 0.75em)
  show heading.where(level: 1): set text(size: 1.45em, tracking: -0.01em)
  show heading.where(level: 1): set block(above: 1.5em, below: 0.7em)
  show heading.where(level: 2): set text(size: 1.2em)
  show heading.where(level: 2): set block(above: 1.55em, below: 0.65em)
  show heading.where(level: 3): set text(size: 1.05em)
  show heading.where(level: 4): set text(size: 1em)
  show heading.where(level: 5): set text(size: 0.95em, fill: theme.muted)
  show heading.where(level: 6): set text(size: 0.9em, fill: theme.muted)

  show: common.base-rules.with(theme: theme)
  body
}

#let md-title(title: none, subtitle: none, author: none, date: none) = {
  block(below: 2.2em, width: 100%, {
    if title != none {
      block(below: 0.85em, text(size: 2.15em, weight: "semibold", tracking: -0.015em, title))
    }
    if subtitle != none {
      block(below: 0.75em, text(size: 1.2em, fill: theme.muted, subtitle))
    }
    let byline = (author, date).filter(x => x != none)
    if byline.len() > 0 {
      block(above: 0.9em, text(size: 0.92em, fill: theme.muted, byline.join([#h(0.5em)·#h(0.5em)])))
    }
    block(above: 1.3em, line(length: 3.2em, stroke: 2pt + theme.accent))
  })
}

#let md-toc(combined: false) = {
  block(below: 0.9em, text(size: 1.2em, weight: "semibold")[Contents])
  set outline.entry(fill: text(fill: theme.rule, repeat(gap: 0.25em)[.]))
  show outline.entry.where(level: 1): set text(weight: "semibold")
  show outline.entry.where(level: 1): set block(above: 1em)
  block(below: 2em, outline(
    title: none,
    depth: if combined { 2 } else { 3 },
    indent: 1.2em,
  ))
  if combined { pagebreak(weak: true) }
}

#let md-cover(title: none) = page(footer: none, {
  v(1fr)
  block(text(size: 2.8em, weight: "semibold", tracking: -0.02em, title))
  block(above: 1.6em, line(length: 4em, stroke: 2.5pt + theme.accent))
  v(2fr)
})

#let md-chapter(title: none, subtitle: none, author: none, date: none) = {
  pagebreak(weak: true)
  if title != none {
    heading(level: 1, title)
    let byline = (subtitle, author, date).filter(x => x != none)
    if byline.len() > 0 {
      block(above: -0.2em, below: 1.6em, text(fill: theme.muted, byline.join([#h(0.5em)·#h(0.5em)])))
    }
  }
}
