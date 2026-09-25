//! What the editor offers while a script is typed: the names that complete the word under the
//! cursor, and the help shown when the pointer rests on a name.
//!
//! This is plain text in, plain text out (no window), so it is tested on its own. The glue that
//! hands it to the editor is in [`super::providers`].

use std::collections::BTreeSet;

use super::lexer::{self, Kind, Place};
use crate::app::chart::study::custom::docs::{self, Group};

/// What an offered name is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateKind {
    Function,
    /// A name the engine provides (`close`).
    Global,
    Keyword,
    /// A block of code that stands for several lines.
    Snippet,
    /// A variable or function the script defines.
    Variable,
    /// A field of what a ready made indicator gives (`bb.upper`).
    Field,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub label: String,
    pub kind: CandidateKind,
    /// One line next to the label: the signature.
    pub detail: String,
    pub documentation: String,
    /// What replaces the word being typed.
    pub insert: String,
}

/// The fields of the maps the ready made indicators give, with what each is.
const FIELDS: &[(&str, &str)] = &[
    ("macd", "The MACD line."),
    ("signal", "The signal line."),
    (
        "hist",
        "The difference between the MACD line and its signal.",
    ),
    ("basis", "The middle line."),
    ("upper", "The upper line or band."),
    ("lower", "The lower line or band."),
    ("k", "The %K line."),
    ("d", "The %D line."),
    ("plus", "+DI."),
    ("minus", "-DI."),
    ("adx", "ADX."),
    ("line", "The line."),
    ("direction", "1 while the trend is up, -1 while it is down."),
];

/// The methods a series has, that are also functions.
const METHODS: &[&str] = &["at", "last", "len", "shift", "set", "to_string"];

/// Blocks of code offered under a short name.
const SNIPPETS: &[(&str, &str, &str)] = &[
    (
        "indicator",
        "indicator(#{ ... })",
        "indicator(#{\n    name: \"My indicator\",\n    short: \"MI\",\n    overlay: true,\n    format: \"price\",\n    category: \"Custom\",\n    description: \"\",\n});\n",
    ),
    ("for", "for i in 0..n { ... }", "for i in 0..n {\n    \n}\n"),
    (
        "fn",
        "fn name(args) { ... }",
        "fn name(a, b) {\n    a + b\n}\n",
    ),
    (
        "if",
        "if condition { ... } else { ... }",
        "if condition {\n    \n} else {\n    \n}\n",
    ),
];

/// The names a script defines with `let`, `const`, `fn` and `for`.
pub fn defined_names(text: &str) -> BTreeSet<String> {
    let tokens = lexer::lex(text);
    let mut names = BTreeSet::new();
    for pair in tokens.windows(2) {
        let first = &text[pair[0].range.clone()];
        if pair[0].kind == Kind::Keyword
            && matches!(first, "let" | "const" | "fn" | "for")
            && matches!(pair[1].kind, Kind::Identifier | Kind::Function)
        {
            names.insert(text[pair[1].range.clone()].to_owned());
        }
    }
    names
}

fn function_candidate(doc: &docs::Doc) -> Candidate {
    Candidate {
        label: doc.name.to_owned(),
        kind: CandidateKind::Function,
        detail: doc.signature.to_owned(),
        documentation: format!("{}\n\nExample: {}", doc.summary, doc.example),
        insert: format!("{}(", doc.name),
    }
}

/// How well `name` answers what was typed: 0 is the best, `None` is no answer.
fn rank(name: &str, typed: &str) -> Option<u8> {
    if typed.is_empty() {
        return Some(2);
    }
    let (name, typed) = (name.to_lowercase(), typed.to_lowercase());
    if name == typed {
        Some(0)
    } else if name.starts_with(&typed) {
        Some(1)
    } else if name.contains(&typed) {
        Some(3)
    } else {
        None
    }
}

/// Among names that answer equally well, the ones of the script and the data come before the
/// functions, and those before the words of the language.
fn kind_order(kind: CandidateKind) -> u8 {
    match kind {
        CandidateKind::Variable => 0,
        CandidateKind::Global | CandidateKind::Field => 1,
        CandidateKind::Function => 2,
        CandidateKind::Snippet => 3,
        CandidateKind::Keyword => 4,
    }
}

/// The most that are offered.
const MAX_CANDIDATES: usize = 40;

/// What can be typed at `offset`: where the word being typed starts, and the names that complete
/// it, best first. Nothing inside a string or a comment.
pub fn candidates(text: &str, offset: usize) -> (usize, Vec<Candidate>) {
    let offset = offset.min(text.len());
    let (start, typed) = lexer::word_before(text, offset);
    if lexer::place_at(text, offset) != Place::Code {
        return (start, Vec::new());
    }
    let mut found: Vec<(u8, u8, Candidate)> = Vec::new();
    let mut add = |order: u8, candidate: Candidate| {
        if let Some(rank) = rank(&candidate.label, typed) {
            found.push((rank, order, candidate));
        }
    };
    if lexer::after_dot(text, offset) {
        for (name, summary) in FIELDS {
            add(
                0,
                Candidate {
                    label: (*name).to_owned(),
                    kind: CandidateKind::Field,
                    detail: "field".to_owned(),
                    documentation: (*summary).to_owned(),
                    insert: (*name).to_owned(),
                },
            );
        }
        for name in METHODS {
            if let Some(doc) = docs::function(name) {
                add(1, function_candidate(doc));
            }
        }
    } else {
        for doc in docs::FUNCTIONS {
            let order = match doc.group {
                Group::Declare => 1,
                _ => 0,
            };
            add(order, function_candidate(doc));
        }
        for global in docs::GLOBALS {
            add(
                0,
                Candidate {
                    label: global.name.to_owned(),
                    kind: CandidateKind::Global,
                    detail: "provided".to_owned(),
                    documentation: global.summary.to_owned(),
                    insert: global.name.to_owned(),
                },
            );
        }
        for word in docs::KEYWORDS {
            add(
                2,
                Candidate {
                    label: (*word).to_owned(),
                    kind: CandidateKind::Keyword,
                    detail: "keyword".to_owned(),
                    documentation: String::new(),
                    insert: (*word).to_owned(),
                },
            );
        }
        for (name, detail, body) in SNIPPETS {
            add(
                2,
                Candidate {
                    label: (*name).to_owned(),
                    kind: CandidateKind::Snippet,
                    detail: (*detail).to_owned(),
                    documentation: format!("Inserts:\n{body}"),
                    insert: (*body).to_owned(),
                },
            );
        }
        // The names the script defines, except the one being typed.
        let own = defined_names(&format!("{} {}", &text[..start], &text[offset..]));
        for name in own {
            add(
                0,
                Candidate {
                    label: name.clone(),
                    kind: CandidateKind::Variable,
                    detail: "defined in this script".to_owned(),
                    documentation: String::new(),
                    insert: name,
                },
            );
        }
    }
    found.sort_by(|a, b| {
        (a.0, kind_order(a.2.kind), a.1, &a.2.label).cmp(&(
            b.0,
            kind_order(b.2.kind),
            b.1,
            &b.2.label,
        ))
    });
    found.dedup_by(|a, b| a.2.label == b.2.label && a.2.kind == b.2.kind);
    (
        start,
        found
            .into_iter()
            .map(|(_, _, c)| c)
            .take(MAX_CANDIDATES)
            .collect(),
    )
}

/// Whether typing `typed` should open the list of names by itself: a letter, a digit or `_` (the
/// word goes on) or a dot (a field is coming).
pub fn triggers(typed: &str) -> bool {
    let mut chars = typed.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => c.is_alphanumeric() || c == '_' || c == '.',
        _ => false,
    }
}

/// The help for the name under `offset`, as text with a bold first line, and the range of the
/// name. Nothing for what the engine does not know.
pub fn hover(text: &str, offset: usize) -> Option<(std::ops::Range<usize>, String)> {
    if lexer::place_at(text, offset) != Place::Code {
        return None;
    }
    let (range, word) = lexer::word_at(text, offset)?;
    if let Some(doc) = docs::function(word) {
        return Some((
            range,
            format!(
                "**{}**\n\n{}\n\n`{}`",
                doc.signature, doc.summary, doc.example
            ),
        ));
    }
    if let Some(global) = docs::global(word) {
        return Some((range, format!("**{}**\n\n{}", global.name, global.summary)));
    }
    if lexer::after_dot(text, range.end)
        && let Some((name, summary)) = FIELDS.iter().find(|(n, _)| *n == word)
    {
        return Some((range, format!("**{name}**\n\n{summary}")));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(text: &str) -> Vec<String> {
        candidates(text, text.len())
            .1
            .into_iter()
            .map(|c| c.label)
            .collect()
    }

    #[test]
    fn a_word_is_completed_with_what_starts_like_it_first() {
        let found = labels("let x = sm");
        assert_eq!(found[0], "sma");
        let (start, list) = candidates("let x = ema(cl", 14);
        assert_eq!(start, 12);
        assert_eq!(list[0].label, "close");
        assert_eq!(list[0].kind, CandidateKind::Global);
    }

    #[test]
    fn a_function_is_completed_with_its_open_bracket_and_its_help() {
        let (_, list) = candidates("plo", 3);
        let plot = list.iter().find(|c| c.label == "plot").unwrap();
        assert_eq!(plot.insert, "plot(");
        assert!(plot.detail.starts_with("plot(key, series"));
        assert!(plot.documentation.contains("Draws a series"));
    }

    #[test]
    fn nothing_is_offered_in_a_string_or_a_comment() {
        assert!(labels("// a comment about sm").is_empty());
        assert!(labels("let s = \"sm").is_empty());
        assert!(!labels("let s = \"a\"; sm").is_empty());
    }

    #[test]
    fn after_a_dot_the_fields_and_the_methods_of_a_series_are_offered() {
        let found = labels("let bb = bollinger(close, 20, 2.0);\nplot(\"u\", bb.up");
        assert_eq!(found[0], "upper");
        assert!(!found.contains(&"sma".to_owned()));
        let methods = labels("close.la");
        assert_eq!(methods[0], "last");
    }

    #[test]
    fn the_names_the_script_defines_are_offered_but_not_the_word_being_typed() {
        let found = labels("let fast_length = 9;\nlet fast_slow = fast_len");
        assert_eq!(found[0], "fast_length");
        assert!(!found.contains(&"fast_len".to_owned()));
        let names = defined_names("let a = 1; const B = 2; fn go(x) { for i in 0..3 {} }");
        assert_eq!(names.into_iter().collect::<Vec<_>>(), ["B", "a", "go", "i"]);
    }

    #[test]
    fn keywords_and_snippets_are_offered() {
        let found = candidates("ind", 3).1;
        assert!(
            found
                .iter()
                .any(|c| c.kind == CandidateKind::Snippet && c.label == "indicator")
        );
        assert!(labels("whi").contains(&"while".to_owned()));
    }

    #[test]
    fn typing_a_letter_or_a_dot_opens_the_list_and_nothing_else_does() {
        assert!(triggers("a") && triggers("_") && triggers(".") && triggers("7"));
        assert!(!triggers(" ") && !triggers("(") && !triggers("ab") && !triggers(""));
    }

    #[test]
    fn hovering_a_name_tells_what_it_is() {
        let text = "plot(\"a\", sma(close, 20));";
        let (range, help) = hover(text, 13).unwrap();
        assert_eq!(&text[range], "sma");
        assert!(help.contains("Simple moving average"));
        assert!(hover(text, 18).unwrap().1.contains("close of every bar"));
        assert!(hover(text, 3).unwrap().1.contains("Draws a series"));
        assert!(hover("// sma", 4).is_none());
        assert!(hover("let unknown = 1;", 6).is_none());
        assert!(hover("x.upper", 4).unwrap().1.contains("upper line"));
    }

    #[test]
    fn every_function_is_offered_by_its_name_and_no_name_is_offered_twice() {
        for doc in docs::FUNCTIONS {
            let (_, list) = candidates(doc.name, doc.name.len());
            assert!(list.iter().any(|c| c.label == doc.name), "{}", doc.name);
        }
        let all = candidates("", 0).1;
        let mut labels: Vec<_> = all.iter().map(|c| (&c.label, c.kind)).collect();
        labels.dedup();
        assert_eq!(labels.len(), all.len());
    }
}
