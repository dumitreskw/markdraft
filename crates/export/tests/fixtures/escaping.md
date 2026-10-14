# Escaping: #hash *star* _under_ \$dollar

Characters that mean something in Typst: # \* \_ \$ @ < > \[ \] \` ~ ' " \\ / : = + -.

#set text(red)
#panic("this must stay text")
#{ 1 + 1 } and #(2 * 3)

= Not a heading

\- escaped dash
\+ escaped plus
1\. escaped number
/ term: not a term list

Comments: // not a comment and /* not a block comment */.

Labels and refs: \<label\> @ref @user.name and email-like a@b.c.

Shorthands: a -- b --- c ... and -? and -1 and ~tilde~.

Math-ish: x_1 + y^2 = \$not math\$ also not.

Quotes: 'single' and "double" and it's.

Backslashes: C:\\Users\\name\\ and \\\\server\\share and a backslash at the end \\

Brackets: \[not a link\] and \]unbalanced\[ and (parens).

Unicode escapes: \\u{1F600} stays literal.

URL in text http://example.com/path?x=1&y=2 (autolinked) and plain example.com.

    #raw code block with $ and # inside
    and "quotes" and \backslashes\

`#inline` code with `$math$`, `\n` and `` ` `` backtick.

Emphasis next to punctuation: *word*.method() and **bold**(paren) and `code`[bracket].
