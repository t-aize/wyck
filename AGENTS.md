# AGENTS.md

## Git commits

Do not add a `Co-Authored-By` line, a session/agent identifier line, or any other AI
assistant attribution to commit messages or pull request descriptions.

## Writing style

Text written for this repository (code comments, doc comments, Markdown files, commit
messages, PR descriptions) must read as plainly typed by a person. Avoid the
typographic characters and habits that mark machine-generated text. They are named by
code point here so that this file itself stays clean:

- No em dash (U+2014) and no en dash (U+2013). Use a comma, a colon, parentheses, or a
  new sentence instead, and reword when the sentence only worked because of the dash.
  Use a plain hyphen-minus for ranges and minus signs, or write "to" ("15 to 30").
- No typographic arrows (U+2190 to U+2193). Write "->" in code-like text, or use words.
- No horizontal ellipsis (U+2026). Write "etc." or three ASCII dots, or reword.
- No curly quotes (U+2018, U+2019, U+201C, U+201D). Use straight quotes.
- No approximately-equal sign (U+2248), multiplication sign (U+00D7), or true minus
  sign (U+2212). Write "about", "x" or "times", and a hyphen-minus.
- No emoji in documentation or comments.
- No invisible characters: zero-width space (U+200B), byte-order mark (U+FEFF),
  bidirectional controls.
- Keep source files ASCII. The one allowed exception is the section sign (U+00A7) when
  citing a section of an external document. Non-ASCII data that a test needs is written
  as an escape sequence (for example `"\u{20AC}"`), so the file itself stays ASCII.

Prose habits to avoid as well: stock filler words ("delve", "leverage", "robust",
"seamless", "comprehensive", "crucial"), announcing structure ("It's worth noting
that"), tacked-on summaries, three-item lists by reflex, and hedging every claim. Say
the concrete thing once, in the fewest words that stay accurate.

Before committing, check for leftovers (prints nothing when clean):

```sh
git ls-files ':!LICENSE' ':!*.lock' ':!*.ttf' \
  | xargs perl -CSD -ne 'print "$ARGV:$.: $_" if /[\x{2013}\x{2014}\x{2018}\x{2019}\x{201C}\x{201D}\x{2026}\x{2190}-\x{2193}\x{2212}\x{D7}\x{2248}\x{200B}\x{FEFF}\x{1F300}-\x{1FAFF}]/'
```
