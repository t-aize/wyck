//! Reading a script as words, for the colors of the editor and for knowing where the cursor is (in
//! a string, in a comment, after a dot).
//!
//! It is not a parser: it only cuts the text into comments, strings, numbers, names and signs, the
//! way Rhai reads them, and never fails. Half typed text gives half a token, so the colors follow
//! what is being written.

use std::ops::Range;

use crate::app::chart::study::custom::docs;

/// What a piece of the text is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Comment,
    String,
    Number,
    /// A word of the language: `let`, `if`, `for`.
    Keyword,
    /// `true`, `false`.
    Bool,
    /// A name the engine provides: `close`, `n`, `na`.
    Global,
    /// A function the engine provides, called: `sma(`.
    Builtin,
    /// A function called or declared by the script.
    Function,
    /// A name after a dot: `bb.upper`.
    Property,
    Identifier,
    /// `+ - * / == && ...`
    Operator,
    /// `( ) [ ] { } , ; : #{`
    Punct,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub range: Range<usize>,
    pub kind: Kind,
}

const OPERATORS: &str = "+-*/%=<>!&|^~?.@";
const PUNCT: &str = "()[]{},;:#";

/// Cuts `text` into tokens. The tokens are in order and never overlap; white space is between
/// them.
pub fn lex(text: &str) -> Vec<Token> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let start = i;
        if c.is_ascii_whitespace() {
            i += 1;
        } else if text[i..].starts_with("//") {
            i = text[i..].find('\n').map_or(text.len(), |n| i + n);
            tokens.push(Token {
                range: start..i,
                kind: Kind::Comment,
            });
        } else if text[i..].starts_with("/*") {
            i = block_comment_end(text, i);
            tokens.push(Token {
                range: start..i,
                kind: Kind::Comment,
            });
        } else if c == b'"' {
            i = string_end(text, i, b'"');
            tokens.push(Token {
                range: start..i,
                kind: Kind::String,
            });
        } else if c == b'`' {
            i = string_end(text, i, b'`');
            tokens.push(Token {
                range: start..i,
                kind: Kind::String,
            });
        } else if c == b'\'' {
            i = string_end(text, i, b'\'');
            tokens.push(Token {
                range: start..i,
                kind: Kind::String,
            });
        } else if c.is_ascii_digit()
            || (c == b'.' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            i = number_end(text, i);
            tokens.push(Token {
                range: start..i,
                kind: Kind::Number,
            });
        } else if is_word_start(text, i) {
            i = word_end(text, i);
            let word = &text[start..i];
            let after_dot = previous_sign(&tokens, text) == Some('.');
            let call = next_sign(text, i) == Some('(');
            let after_fn = previous_word(&tokens, text) == Some("fn");
            let kind = if after_dot {
                Kind::Property
            } else if after_fn {
                Kind::Function
            } else if word == "true" || word == "false" {
                Kind::Bool
            } else if call && is_call_word(word) {
                Kind::Builtin
            } else if docs::KEYWORDS.contains(&word) {
                Kind::Keyword
            } else if call && docs::function(word).is_some() {
                Kind::Builtin
            } else if call {
                Kind::Function
            } else if docs::global(word).is_some() {
                Kind::Global
            } else {
                Kind::Identifier
            };
            tokens.push(Token {
                range: start..i,
                kind,
            });
        } else if text[i..].starts_with("#{") {
            i += 2;
            tokens.push(Token {
                range: start..i,
                kind: Kind::Punct,
            });
        } else if OPERATORS.contains(c as char) {
            i += 1;
            while i < bytes.len() && OPERATORS.contains(bytes[i] as char) && bytes[i] != b'/' {
                i += 1;
            }
            tokens.push(Token {
                range: start..i,
                kind: Kind::Operator,
            });
        } else if PUNCT.contains(c as char) {
            i += 1;
            tokens.push(Token {
                range: start..i,
                kind: Kind::Punct,
            });
        } else {
            // Anything else (a stray character, part of a multi byte letter): one whole character.
            i += text[i..].chars().next().map_or(1, char::len_utf8);
            tokens.push(Token {
                range: start..i,
                kind: Kind::Identifier,
            });
        }
    }
    tokens
}

/// `print(` and `debug(` are keywords of the language that are also called.
fn is_call_word(word: &str) -> bool {
    matches!(word, "print" | "debug")
}

fn is_word_start(text: &str, at: usize) -> bool {
    text[at..]
        .chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_')
}

fn word_end(text: &str, from: usize) -> usize {
    let rest = &text[from..];
    from + rest
        .char_indices()
        .find(|(_, c)| !(c.is_alphanumeric() || *c == '_'))
        .map_or(rest.len(), |(n, _)| n)
}

fn number_end(text: &str, from: usize) -> usize {
    let bytes = text.as_bytes();
    let mut i = from;
    if text[i..].starts_with("0x") || text[i..].starts_with("0b") || text[i..].starts_with("0o") {
        i += 2;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
        }
        return i;
    }
    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
        i += 1;
    }
    // A decimal part, but not the `..` of a range or a method call on a number.
    if bytes.get(i) == Some(&b'.') && bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
        i += 1;
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
            i += 1;
        }
    }
    if matches!(bytes.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(bytes.get(j), Some(b'+' | b'-')) {
            j += 1;
        }
        if bytes.get(j).is_some_and(u8::is_ascii_digit) {
            i = j;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
        }
    }
    i
}

/// The end of a quoted piece that starts at `from` (on the quote): after the closing quote, or at
/// the end of the text when it is not closed. A `\` skips the next character. Only a backtick
/// string may go over lines.
fn string_end(text: &str, from: usize, quote: u8) -> usize {
    let bytes = text.as_bytes();
    let mut i = from + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b if b == quote => return i + 1,
            b'\n' if quote != b'`' => return i,
            _ => i += 1,
        }
    }
    text.len()
}

/// The end of a block comment that starts at `from`. Comments nest, as they do in Rhai.
fn block_comment_end(text: &str, from: usize) -> usize {
    let mut depth = 0usize;
    let mut i = from;
    while i < text.len() {
        if text[i..].starts_with("/*") {
            depth += 1;
            i += 2;
        } else if text[i..].starts_with("*/") {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return i;
            }
        } else {
            i += text[i..].chars().next().map_or(1, char::len_utf8);
        }
    }
    text.len()
}

/// `Some('.')` when the last token is a dot (member access), not the `..` of a range.
fn previous_sign(tokens: &[Token], text: &str) -> Option<char> {
    let last = tokens.last()?;
    (last.kind == Kind::Operator && matches!(&text[last.range.clone()], "." | "?.")).then_some('.')
}

fn previous_word<'a>(tokens: &[Token], text: &'a str) -> Option<&'a str> {
    let last = tokens.last()?;
    (last.kind == Kind::Keyword).then(|| &text[last.range.clone()])
}

/// The next character that is not white space, from `from`.
fn next_sign(text: &str, from: usize) -> Option<char> {
    text[from..].chars().find(|c| !c.is_whitespace())
}

/// Where a position of the text is, for what the editor offers there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// Somewhere code can be written.
    Code,
    /// Inside a string.
    String,
    /// Inside a comment.
    Comment,
}

/// What `offset` is in: a comment, a string, or code.
pub fn place_at(text: &str, offset: usize) -> Place {
    for token in lex(text) {
        if token.range.start >= offset {
            break;
        }
        // An unclosed string or comment runs to the end of its line or the text, and the cursor
        // at its end is still inside it; a closed one is over at its end.
        let closed = match token.kind {
            Kind::String => {
                let quote = text.as_bytes()[token.range.start];
                token.range.len() >= 2 && text.as_bytes()[token.range.end - 1] == quote
            }
            // A line comment runs to the end of its line: the cursor at its end is still in it.
            Kind::Comment => {
                !text[token.range.clone()].starts_with("//")
                    && text[token.range.clone()].ends_with("*/")
            }
            _ => continue,
        };
        let inside = offset < token.range.end || (!closed && offset <= token.range.end);
        // A line comment ends before its newline.
        if inside {
            return if token.kind == Kind::String {
                Place::String
            } else {
                Place::Comment
            };
        }
    }
    Place::Code
}

/// The word that ends at `offset` (what is being typed), and where it starts.
pub fn word_before(text: &str, offset: usize) -> (usize, &str) {
    let offset = offset.min(text.len());
    let head = &text[..offset];
    let start = head
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphanumeric() || *c == '_')
        .last()
        .map_or(offset, |(n, _)| n);
    (start, &text[start..offset])
}

/// Whether the word that ends at `offset` comes right after a dot (a property of a value).
pub fn after_dot(text: &str, offset: usize) -> bool {
    let (start, _) = word_before(text, offset);
    text[..start].trim_end_matches([' ', '\t']).ends_with('.')
}

/// The whole word around `offset`, with where it is.
pub fn word_at(text: &str, offset: usize) -> Option<(Range<usize>, &str)> {
    let offset = offset.min(text.len());
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let start = text[..offset]
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_word(*c))
        .last()
        .map_or(offset, |(n, _)| n);
    let end = offset
        + text[offset..]
            .char_indices()
            .find(|(_, c)| !is_word(*c))
            .map_or(text.len() - offset, |(n, _)| n);
    (start < end).then(|| (start..end, &text[start..end]))
}

/// The lines (from 0) of the blocks between `{` and `}` that go over several lines, for folding.
pub fn blocks(text: &str) -> Vec<(usize, usize)> {
    let mut stack: Vec<usize> = Vec::new();
    let mut out = Vec::new();
    let mut line = 0usize;
    let mut last = 0usize;
    for token in lex(text) {
        line += text[last..token.range.start].matches('\n').count();
        last = token.range.start;
        if token.kind == Kind::Punct {
            let piece = &text[token.range.clone()];
            if piece.ends_with('{') {
                stack.push(line);
            } else if piece == "}"
                && let Some(open) = stack.pop()
                && open < line
            {
                out.push((open, line));
            }
        }
    }
    out.sort_unstable();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<(&str, Kind)> {
        lex(text)
            .into_iter()
            .map(|t| (&text[t.range], t.kind))
            .collect()
    }

    #[test]
    fn a_line_of_a_script_is_cut_into_what_it_is_made_of() {
        assert_eq!(
            kinds("let x = sma(close, 20); // average"),
            [
                ("let", Kind::Keyword),
                ("x", Kind::Identifier),
                ("=", Kind::Operator),
                ("sma", Kind::Builtin),
                ("(", Kind::Punct),
                ("close", Kind::Global),
                (",", Kind::Punct),
                ("20", Kind::Number),
                (")", Kind::Punct),
                (";", Kind::Punct),
                ("// average", Kind::Comment),
            ]
        );
    }

    #[test]
    fn strings_come_in_three_kinds_and_may_not_be_closed() {
        assert_eq!(kinds("\"a\\\"b\"")[0], ("\"a\\\"b\"", Kind::String));
        assert_eq!(kinds("`x ${y}`")[0].1, Kind::String);
        assert_eq!(kinds("'c'")[0].1, Kind::String);
        assert_eq!(kinds("\"open")[0], ("\"open", Kind::String));
        // A string stops at the end of its line, so one open quote does not swallow the script.
        let tokens = kinds("\"open\nlet a");
        assert_eq!(tokens[1], ("let", Kind::Keyword));
        // A backtick string may go over lines.
        assert_eq!(kinds("`a\nb`").len(), 1);
    }

    #[test]
    fn numbers_are_read_whole_and_a_range_is_not_a_decimal() {
        for text in ["12", "1_000", "0.5", ".5", "1e5", "2.5e-3", "0xff"] {
            let tokens = kinds(text);
            assert_eq!(tokens, [(text, Kind::Number)], "{text}");
        }
        let range = kinds("0..n");
        assert_eq!(range[0], ("0", Kind::Number));
        assert_eq!(range[1].1, Kind::Operator);
        assert_eq!(range[2], ("n", Kind::Global));
    }

    #[test]
    fn block_comments_nest_and_may_be_left_open() {
        assert_eq!(kinds("/* a /* b */ c */ x").len(), 2);
        assert_eq!(kinds("/* open")[0].1, Kind::Comment);
    }

    #[test]
    fn names_are_told_apart_by_what_follows_and_what_comes_before() {
        let tokens = kinds("fn mine(x) { bb.upper + rsi(x, 14) + mine(3) + true }");
        assert!(tokens.contains(&("mine", Kind::Function)));
        assert!(tokens.contains(&("upper", Kind::Property)));
        assert!(tokens.contains(&("rsi", Kind::Builtin)));
        assert!(tokens.contains(&("true", Kind::Bool)));
        assert_eq!(
            tokens
                .iter()
                .filter(|t| **t == ("mine", Kind::Function))
                .count(),
            2
        );
        assert_eq!(kinds("#{ a: 1 }")[0], ("#{", Kind::Punct));
        assert_eq!(kinds("print(x)")[0].1, Kind::Builtin);
        assert_eq!(kinds("print")[0].1, Kind::Keyword);
    }

    #[test]
    fn the_tokens_never_overlap_and_the_lexer_survives_anything() {
        let text = "let \u{e9} = \"\u{e9}\"; /* \u{fc} */ x ?? ::: 0x /*";
        let mut end = 0;
        for token in lex(text) {
            assert!(token.range.start >= end && token.range.end > token.range.start);
            assert!(
                text.is_char_boundary(token.range.start) && text.is_char_boundary(token.range.end)
            );
            end = token.range.end;
        }
        assert!(lex("").is_empty());
    }

    #[test]
    fn the_place_of_a_position_says_whether_code_can_be_written_there() {
        let text = "let a = 1; // note\nlet s = \"str\"; plot";
        assert_eq!(place_at(text, 3), Place::Code);
        assert_eq!(place_at(text, 14), Place::Comment);
        assert_eq!(
            place_at(text, 18),
            Place::Comment,
            "the end of the comment, before the newline"
        );
        assert_eq!(place_at(text, 19), Place::Code);
        assert_eq!(place_at(text, 28), Place::String);
        assert_eq!(place_at(text, text.len()), Place::Code);
        assert_eq!(place_at("let s = \"open", 13), Place::String);
    }

    #[test]
    fn the_word_being_typed_and_the_word_under_the_cursor_are_found() {
        let text = "let bb = bollinger(cl";
        assert_eq!(word_before(text, text.len()), (19, "cl"));
        assert_eq!(word_before("x.", 2), (2, ""));
        assert!(after_dot("bb.up", 5) && !after_dot("bb up", 5));
        assert_eq!(word_at("plot(abc)", 6), Some((5..8, "abc")));
        assert_eq!(word_at("plot(abc)", 5).map(|w| w.1), Some("abc"));
        assert_eq!(word_at("a + b", 2), None);
    }

    #[test]
    fn a_block_over_several_lines_can_be_folded() {
        let text = "fn a() {\n  if x {\n    y\n  }\n}\nlet m = #{ a: 1 };\nlet n = #{\n a: 2\n};";
        assert_eq!(blocks(text), [(0, 4), (1, 3), (6, 8)]);
    }
}
