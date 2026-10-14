// Typesets LaTeX math that mitex converted to Typst math code.
#import "mitex/mod.typ": mitex-scope

#let md-math(src, block: false) = math.equation(
  block: block,
  eval(src, mode: "math", scope: mitex-scope),
)
