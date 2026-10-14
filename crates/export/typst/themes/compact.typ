// MarkDraft theme "compact": Inter at a small size, tight margins and
// spacing for dense reference material.
#import "/markdraft/common.typ" as common

#let theme = (
  ink: rgb("#1c2024"),
  muted: rgb("#606b75"),
  accent: rgb("#2f6f62"),
  link: rgb("#2a6a8f"),
  rule: rgb("#d5dbe0"),
  code-bg: rgb("#f4f6f7"),
  code-border: rgb("#e1e6ea"),
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
    margin: (x: 1.6cm, top: 1.6cm, bottom: 1.9cm),
    footer: common.page-number(theme: theme),
    footer-descent: 35%,
  )
  set text(font: "Inter", size: 9.5pt, fill: theme.ink, lang: "en", hyphenate: false)
  set par(leading: 0.58em, spacing: 0.85em, justify: false)
  set list(indent: 0.2em, body-indent: 0.45em)
  set enum(indent: 0.2em, body-indent: 0.45em)

  set heading(numbering: none)
  show heading: set text(weight: "semibold", fill: theme.ink)
  show heading: set block(above: 1.35em, below: 0.6em)
  show heading.where(level: 1): set text(size: 1.4em)
  show heading.where(level: 2): set text(size: 1.18em)
  show heading.where(level: 3): set text(size: 1.05em)
  show heading.where(level: 4): set text(size: 1em)
  show heading.where(level: 5): set text(size: 0.95em, fill: theme.muted)
  show heading.where(level: 6): set text(size: 0.9em, fill: theme.muted)

  show: common.base-rules.with(theme: theme)
  body
}

#let md-title(title: none, subtitle: none, author: none, date: none) = {
  block(below: 1.4em, width: 100%, {
    if title != none {
      block(below: 0.65em, text(size: 1.85em, weight: "semibold", tracking: -0.01em, title))
    }
    if subtitle != none {
      block(below: 0.45em, text(size: 1.1em, fill: theme.muted, subtitle))
    }
    let byline = (author, date).filter(x => x != none)
    if byline.len() > 0 {
      block(above: 0.6em, text(size: 0.92em, fill: theme.muted, byline.join([#h(0.5em)·#h(0.5em)])))
    }
    block(above: 0.9em, line(length: 100%, stroke: 0.6pt + theme.rule))
  })
}

#let md-toc(combined: false) = {
  block(below: 0.7em, text(size: 1.12em, weight: "semibold")[Contents])
  set outline.entry(fill: text(fill: theme.rule, repeat(gap: 0.25em)[.]))
  show outline.entry.where(level: 1): set text(weight: "semibold")
  block(below: 1.4em, outline(
    title: none,
    depth: if combined { 2 } else { 3 },
    indent: 1em,
  ))
  if combined { pagebreak(weak: true) }
}

#let md-cover(title: none) = page(footer: none, {
  v(1fr)
  block(text(size: 2.4em, weight: "semibold", tracking: -0.02em, title))
  block(above: 1.2em, line(length: 100%, stroke: 0.6pt + theme.rule))
  v(2fr)
})

#let md-chapter(title: none, subtitle: none, author: none, date: none) = {
  pagebreak(weak: true)
  if title != none {
    heading(level: 1, title)
    let byline = (subtitle, author, date).filter(x => x != none)
    if byline.len() > 0 {
      block(above: -0.2em, below: 1.2em, text(fill: theme.muted, byline.join([#h(0.5em)·#h(0.5em)])))
    }
  }
}
