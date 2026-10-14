// Components shared by all MarkDraft export themes.
//
// Generated documents call the `md-*` functions; each theme binds them to
// its palette with `.with(theme: ...)` and adds its own set/show rules in
// `setup`. A theme dictionary has these keys:
//   ink, muted, accent, rule, code-bg, code-border, link,
//   body-font, heading-font, mono-font

#let mono-font = "JetBrains Mono NL"
#let math-font = "New Computer Modern Math"

#let alert-styles = (
  note: (color: rgb("#0b62c4"), tint: rgb("#f0f6fe"), label: "Note"),
  tip: (color: rgb("#1f7a3d"), tint: rgb("#eff8f1"), label: "Tip"),
  important: (color: rgb("#7343c9"), tint: rgb("#f5f1fc"), label: "Important"),
  warning: (color: rgb("#9a5b00"), tint: rgb("#fdf6e7"), label: "Warning"),
  caution: (color: rgb("#c0262d"), tint: rgb("#fdf1f1"), label: "Caution"),
)

/// Shared set/show rules: code, quotes, tables, links, footnotes, math.
#let base-rules(theme: none, body) = {
  set raw(tab-size: 4)
  // Typst already scales raw text to 0.8em and em sizes compound, so these
  // give 0.88em (inline) and 0.85em (blocks) of the surrounding text.
  show raw: set text(font: mono-font, ligatures: false)
  show raw.where(block: false): set text(size: 0.88em / 0.8)
  show raw.where(block: true): set text(size: 0.85em / 0.8)
  show raw.where(block: true): set par(justify: false, leading: 0.6em)
  show math.equation: set text(font: math-font)

  // Inline code: subtle pill.
  show raw.where(block: false): it => box(
    fill: theme.code-bg,
    inset: (x: 0.2em),
    outset: (y: 0.28em),
    radius: 0.22em,
    it,
  )
  // Code blocks: tinted panel that may break across pages.
  show raw.where(block: true): it => block(
    width: 100%,
    fill: theme.code-bg,
    stroke: 0.5pt + theme.code-border,
    inset: (x: 1em, y: 0.85em),
    radius: 3pt,
    breakable: true,
    it,
  )

  // Tight lists (Markdown lists without blank lines between items) keep
  // nested lists and paragraphs inside items at line spacing too.
  show selector.or(list.where(tight: true), enum.where(tight: true)): it => context {
    let outer = par.spacing
    block(above: outer, below: outer, {
      set par(spacing: par.leading)
      set block(spacing: par.leading)
      it
    })
  }

  show link: set text(fill: theme.link)

  // Block quotes: muted text with a rule on the left.
  show quote.where(block: true): it => block(
    width: 100%,
    inset: (left: 1em, y: 0.15em),
    stroke: (left: 2pt + theme.rule),
    text(fill: theme.muted, it.body),
  )

  // Tables: no vertical rules, a firm rule under the header, hairlines
  // between rows.
  set table(
    inset: (x: 0.65em, y: 0.5em),
    fill: (_, y) => if y == 0 { theme.code-bg },
    stroke: (_, y) => (
      top: if y == 0 { 0.8pt + theme.ink } else { none },
      bottom: if y == 0 { 0.8pt + theme.ink } else { 0.4pt + theme.rule },
    ),
  )
  show table: set text(size: 0.92em)
  show table: set par(justify: false)
  show table.cell.where(y: 0): set text(weight: "semibold")
  show table: it => block(width: 100%, breakable: true, it)

  set footnote.entry(separator: line(length: 25%, stroke: 0.5pt + theme.rule), gap: 0.5em)
  show footnote.entry: set text(size: 0.84em)
  show footnote.entry: set par(justify: false)

  set list(marker: (text(fill: theme.muted)[•], text(fill: theme.muted)[◦], text(fill: theme.muted)[–]))

  body
}

/// GitHub-style callout (`> [!NOTE]` ...).
#let md-alert(kind, title: none, theme: none, body) = {
  let style = alert-styles.at(kind)
  block(
    width: 100%,
    fill: style.tint,
    stroke: (left: 2.5pt + style.color),
    inset: (left: 1em, right: 1em, top: 0.7em, bottom: 0.8em),
    radius: (right: 3pt),
    breakable: true,
    {
      block(
        below: 0.65em,
        text(
          fill: style.color,
          weight: "semibold",
          size: 0.82em,
          tracking: 0.06em,
          upper(if title == none { style.label } else { title }),
        ),
      )
      body
    },
  )
}

#let checkbox(checked, theme: none) = box(
  width: 0.78em,
  height: 0.78em,
  baseline: 0.1em,
  radius: 0.16em,
  stroke: 0.6pt + if checked { theme.accent } else { theme.muted },
  fill: if checked { theme.accent } else { none },
  if checked {
    place(curve(
      stroke: (paint: white, thickness: 0.11em, cap: "round", join: "round"),
      curve.move((0.19em, 0.41em)),
      curve.line((0.34em, 0.56em)),
      curve.line((0.6em, 0.24em)),
    ))
  },
)

/// Task list: items are `(state, body)` where state is `true` (done),
/// `false` (open) or `none` (a plain bullet inside a task list).
#let md-tasks(tight: true, theme: none, ..items) = context {
  let outer = par.spacing
  let gap = if tight { par.leading } else { par.spacing }
  block(above: outer, below: outer, {
    set par(spacing: gap)
    set block(spacing: gap)
    pad(left: list.indent, grid(
      columns: (auto, 1fr),
      column-gutter: 0.55em,
      row-gutter: gap,
      ..items.pos().map(((state, body)) => (
        if state == none { text(fill: theme.muted)[•] } else { checkbox(state, theme: theme) },
        body,
      )).flatten()
    ))
  })
}

/// Scales an image down so it fits the available width and a share of the
/// page height; never scales up.
#let fit-image(path, alt, width) = layout(size => context {
  let img = image(path, alt: alt, width: width)
  let natural = measure(img)
  let max-height = page.height * 0.62
  let scale = calc.min(
    1.0,
    if natural.width > 0pt { size.width / natural.width } else { 1.0 },
    if natural.height > 0pt { max-height / natural.height } else { 1.0 },
  )
  if scale < 1.0 { image(path, alt: alt, width: natural.width * scale) } else { img }
})

/// An image; `width` is its natural display width (auto for SVG).
#let md-image(path, alt: none, width: auto, caption: none, block: false, theme: none) = {
  let alt = if alt == "" { none } else { alt }
  if block {
    std.block(width: 100%, above: 1.4em, below: 1.4em, align(center, {
      fit-image(path, alt, width)
      if caption != none {
        std.block(above: 0.7em, text(size: 0.85em, fill: theme.muted, caption))
      }
    }))
  } else {
    box(fit-image(path, alt, width))
  }
}

#let md-diagram(path, theme: none) = md-image(path, block: true, theme: theme)

#let md-missing-image(src, alt: none, block: false, theme: none) = {
  let label = if alt != none and alt != "" { alt } else { src }
  let chip = box(
    inset: (x: 0.4em, y: 0.15em),
    stroke: 0.5pt + theme.rule,
    radius: 2pt,
    text(size: 0.82em, style: "italic", fill: theme.muted)[Image: #label],
  )
  if block { std.block(chip) } else { chip }
}

/// LaTeX shown verbatim (math that could not be converted).
#let md-math-source(src, block: false, theme: none) = {
  if block {
    std.block(
      width: 100%,
      fill: theme.code-bg,
      stroke: (left: 2pt + theme.rule),
      inset: (x: 0.9em, y: 0.6em),
      text(size: 0.86em, raw(src, lang: "latex")),
    )
  } else {
    raw(src)
  }
}

#let md-rule(theme: none) = block(
  above: 1.6em,
  below: 1.6em,
  line(length: 100%, stroke: 0.6pt + theme.rule),
)

/// Page footer with a centered page number.
#let page-number(theme: none) = context align(center, text(
  size: 0.8em,
  fill: theme.muted,
  counter(page).display("1"),
))
