// MarkDraft theme "academic": Source Serif 4, justified text, numbered
// headings, running header.
#import "/markdraft/common.typ" as common

#let theme = (
  ink: rgb("#1a1a1a"),
  muted: rgb("#5c5c5c"),
  accent: rgb("#7a2e2e"),
  link: rgb("#1f4e8c"),
  rule: rgb("#cfcfcf"),
  code-bg: rgb("#f6f6f4"),
  code-border: rgb("#e4e4df"),
)

#let md-alert = common.md-alert.with(theme: theme)
#let md-tasks = common.md-tasks.with(theme: theme)
#let md-image = common.md-image.with(theme: theme)
#let md-diagram = common.md-diagram.with(theme: theme)
#let md-missing-image = common.md-missing-image.with(theme: theme)
#let md-math-source = common.md-math-source.with(theme: theme)
#let md-rule = common.md-rule.with(theme: theme)

#let serif = "Source Serif 4"

// Numbers the first three heading levels: 1, 1.1, 1.1.1.
#let heading-numbering(..nums) = {
  let n = nums.pos()
  if n.len() <= 3 { numbering("1.1", ..n) }
}

#let running-header(title) = context {
  if title != none and here().page() > 1 {
    set text(size: 0.8em, fill: theme.muted, style: "italic")
    align(center, title)
  }
}

#let md-setup(paper: "a4", title: none, author: none, body) = {
  set document(title: title, author: if author != none { author } else { () }, date: none)
  set page(
    paper: paper,
    margin: (x: 2.6cm, top: 2.8cm, bottom: 2.8cm),
    header: running-header(title),
    footer: common.page-number(theme: theme),
    footer-descent: 40%,
  )
  set text(font: serif, size: 11pt, fill: theme.ink, lang: "en", hyphenate: true)
  set par(justify: true, leading: 0.66em, spacing: 1.05em)
  set list(indent: 0.4em, body-indent: 0.55em)
  set enum(indent: 0.4em, body-indent: 0.55em)

  set heading(numbering: heading-numbering)
  show heading: set text(font: serif, weight: "bold", fill: theme.ink)
  show heading: set block(above: 1.7em, below: 0.8em)
  show heading: set par(justify: false)
  show heading.where(level: 1): set text(size: 1.35em)
  show heading.where(level: 2): set text(size: 1.15em)
  show heading.where(level: 3): set text(size: 1.02em)
  show heading.where(level: 4): set text(size: 1em, style: "italic")
  show heading.where(level: 5): set text(size: 1em, weight: "regular", style: "italic")
  show heading.where(level: 6): set text(size: 1em, weight: "regular", style: "italic")

  show: common.base-rules.with(theme: theme)
  body
}

#let md-title(title: none, subtitle: none, author: none, date: none) = {
  align(center, block(below: 2.4em, width: 100%, {
    if title != none {
      block(below: 0.7em, par(justify: false, text(size: 1.9em, weight: "bold", title)))
    }
    if subtitle != none {
      block(below: 0.7em, text(size: 1.2em, style: "italic", subtitle))
    }
    if author != none {
      block(above: 1.1em, text(size: 1.05em, author))
    }
    if date != none {
      block(above: 0.5em, text(size: 0.95em, fill: theme.muted, date))
    }
    block(above: 1.4em, line(length: 30%, stroke: 0.5pt + theme.ink))
  }))
}

#let md-toc(combined: false) = {
  block(below: 0.9em, text(size: 1.25em, weight: "bold")[Contents])
  set outline.entry(fill: text(fill: theme.rule, repeat(gap: 0.25em)[.]))
  show outline.entry.where(level: 1): set text(weight: "bold")
  show outline.entry.where(level: 1): set block(above: 0.9em)
  block(below: 2em, outline(
    title: none,
    depth: if combined { 2 } else { 3 },
    indent: auto,
  ))
  if combined { pagebreak(weak: true) }
}

#let md-cover(title: none) = page(header: none, footer: none, {
  v(1fr)
  align(center, {
    block(par(justify: false, text(size: 2.6em, weight: "bold", title)))
    block(above: 1.6em, line(length: 25%, stroke: 0.6pt + theme.ink))
  })
  v(1.6fr)
})

#let md-chapter(title: none, subtitle: none, author: none, date: none) = {
  pagebreak(weak: true)
  if title != none {
    heading(level: 1, title)
    let byline = (subtitle, author, date).filter(x => x != none)
    if byline.len() > 0 {
      block(above: -0.2em, below: 1.6em, text(style: "italic", fill: theme.muted, byline.join([ · ])))
    }
  }
}
