//! What connects the words of a script to the editor of gpui: the colors, the folds, the list of
//! names that opens while typing, and the help that shows under the pointer. The thinking is in
//! [`super::lexer`] and [`super::assist`]; this only hands it over in the shape the editor takes.

use std::ops::Range;
use std::rc::Rc;

use gpui::{App, Context, FontStyle, FontWeight, HighlightStyle, Hsla, SharedString, Task, Window};
use gpui_kit::component::input::{
    CompletionProvider, EditorState, FoldRange, HighlightStyleResolver, HoverProvider, InputEdit,
    InputHighlighter, InputHighlighterFactory, Rope, RopeExt,
};
use lsp_types::{
    CompletionContext, CompletionItem, CompletionItemKind, CompletionResponse, CompletionTextEdit,
    Documentation, Hover, HoverContents, MarkupContent, MarkupKind, TextEdit,
};

use super::assist::{self, CandidateKind};
use super::lexer::{self, Kind, Token};
use crate::app::theme;

/// The name of the language, as the editor is told.
pub const LANGUAGE: &str = "rhai";

// ---- colors ----

fn hsla(color: gpui::Rgba) -> Hsla {
    color.into()
}

/// How a kind of word is drawn: from the colors of the theme, so it follows the theme.
pub fn style_of(kind: Kind) -> HighlightStyle {
    let color = |c: gpui::Rgba| HighlightStyle {
        color: Some(hsla(c)),
        ..HighlightStyle::default()
    };
    match kind {
        Kind::Comment => HighlightStyle {
            color: Some(hsla(theme::muted_fg())),
            font_style: Some(FontStyle::Italic),
            ..HighlightStyle::default()
        },
        Kind::String => color(theme::emerald()),
        Kind::Number | Kind::Bool => color(theme::amber()),
        Kind::Keyword => HighlightStyle {
            color: Some(hsla(theme::accent())),
            font_weight: Some(FontWeight::MEDIUM),
            ..HighlightStyle::default()
        },
        Kind::Global => color(theme::chart_line()),
        Kind::Builtin => HighlightStyle {
            color: Some(hsla(theme::chart_line())),
            font_weight: Some(FontWeight::MEDIUM),
            ..HighlightStyle::default()
        },
        Kind::Function => HighlightStyle {
            color: Some(hsla(theme::accent())),
            ..HighlightStyle::default()
        },
        Kind::Property => color(theme::fg_alpha(0.85)),
        Kind::Operator | Kind::Punct => color(theme::muted_fg()),
        Kind::Identifier => HighlightStyle::default(),
    }
}

/// The coloring of a script: it reads the whole text again at every change, which is nothing for
/// a script (they are limited to 256 KB).
#[derive(Default)]
pub struct Highlighter {
    tokens: Vec<Token>,
    blocks: Vec<(usize, usize)>,
}

impl InputHighlighter for Highlighter {
    fn language(&self) -> SharedString {
        SharedString::from(LANGUAGE)
    }

    fn update(
        &mut self,
        _edit: Option<InputEdit>,
        text: &Rope,
        folding: bool,
        _window: &mut Window,
        _cx: &mut Context<EditorState>,
    ) {
        let source = text.to_string();
        self.tokens = lexer::lex(&source);
        self.blocks = if folding {
            lexer::blocks(&source)
        } else {
            Vec::new()
        };
    }

    fn styles(
        &self,
        range: &Range<usize>,
        _resolver: &dyn HighlightStyleResolver,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        let mut out = Vec::new();
        let mut at = range.start;
        // The first token that ends after the start of the range.
        let first = self.tokens.partition_point(|t| t.range.end <= range.start);
        for token in &self.tokens[first..] {
            if token.range.start >= range.end {
                break;
            }
            let (start, end) = (
                token.range.start.max(range.start),
                token.range.end.min(range.end),
            );
            if start > at {
                out.push((at..start, HighlightStyle::default()));
            }
            out.push((start..end, style_of(token.kind)));
            at = end;
        }
        if at < range.end {
            out.push((at..range.end, HighlightStyle::default()));
        }
        out
    }

    fn fold_ranges(&self, _text: &Rope) -> Vec<FoldRange> {
        self.blocks
            .iter()
            .map(|(start, end)| FoldRange::new(*start, *end))
            .collect()
    }
}

/// What the editor asks for a highlighter by language name.
pub fn highlighter_factory() -> InputHighlighterFactory {
    Rc::new(|language| {
        language
            .eq_ignore_ascii_case(LANGUAGE)
            .then(|| Box::new(Highlighter::default()) as Box<dyn InputHighlighter>)
    })
}

// ---- completion ----

pub struct Completions;

fn item_kind(kind: CandidateKind) -> CompletionItemKind {
    match kind {
        CandidateKind::Function => CompletionItemKind::FUNCTION,
        CandidateKind::Global => CompletionItemKind::CONSTANT,
        CandidateKind::Keyword => CompletionItemKind::KEYWORD,
        CandidateKind::Snippet => CompletionItemKind::SNIPPET,
        CandidateKind::Variable => CompletionItemKind::VARIABLE,
        CandidateKind::Field => CompletionItemKind::FIELD,
    }
}

impl CompletionProvider for Completions {
    fn completions(
        &self,
        text: &Rope,
        offset: usize,
        _trigger: CompletionContext,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Task<anyhow::Result<CompletionResponse>> {
        let source = text.to_string();
        let (start, found) = assist::candidates(&source, offset);
        let range = lsp_types::Range {
            start: text.offset_to_position(start),
            end: text.offset_to_position(offset),
        };
        let items = found
            .into_iter()
            .map(|c| CompletionItem {
                label: c.label,
                kind: Some(item_kind(c.kind)),
                detail: Some(c.detail),
                documentation: (!c.documentation.is_empty())
                    .then_some(Documentation::String(c.documentation)),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range,
                    new_text: c.insert,
                })),
                ..CompletionItem::default()
            })
            .collect();
        Task::ready(Ok(CompletionResponse::Array(items)))
    }

    fn is_completion_trigger(&self, _offset: usize, new_text: &str, _cx: &mut App) -> bool {
        assist::triggers(new_text)
    }
}

// ---- help under the pointer ----

pub struct Help;

impl HoverProvider for Help {
    fn hover(
        &self,
        text: &Rope,
        offset: usize,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Task<anyhow::Result<Option<Hover>>> {
        let source = text.to_string();
        let hover = assist::hover(&source, offset).map(|(range, markdown)| Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: markdown,
            }),
            range: Some(lsp_types::Range {
                start: text.offset_to_position(range.start),
                end: text.offset_to_position(range.end),
            }),
        });
        Task::ready(Ok(hover))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoStyles;
    impl HighlightStyleResolver for NoStyles {
        fn style(&self, _name: &str) -> Option<HighlightStyle> {
            None
        }
    }

    fn highlighter(text: &str) -> Highlighter {
        Highlighter {
            tokens: lexer::lex(text),
            blocks: lexer::blocks(text),
        }
    }

    #[test]
    fn the_styles_of_a_range_cover_it_completely_without_gaps_or_overlaps() {
        let text = "let x = sma(close, 20); // note\nplot(\"a\", x);";
        let h = highlighter(text);
        for range in [0..text.len(), 4..14, 12..12 + 3, 30..text.len(), 0..1] {
            let runs = h.styles(&range, &NoStyles);
            assert_eq!(
                runs.first().map(|r| r.0.start),
                Some(range.start),
                "{range:?}"
            );
            assert_eq!(runs.last().map(|r| r.0.end), Some(range.end), "{range:?}");
            for pair in runs.windows(2) {
                assert_eq!(pair[0].0.end, pair[1].0.start, "{range:?}");
            }
        }
    }

    #[test]
    fn a_keyword_a_string_and_a_comment_are_drawn_differently() {
        let text = "let s = \"a\"; // c";
        let h = highlighter(text);
        let runs = h.styles(&(0..text.len()), &NoStyles);
        let color_of = |at: usize| {
            runs.iter()
                .find(|(r, _)| r.contains(&at))
                .and_then(|(_, s)| s.color)
        };
        assert_ne!(color_of(0), color_of(9));
        assert_ne!(color_of(9), color_of(14));
        assert_eq!(color_of(4), None, "a plain name is drawn in the text color");
    }

    #[test]
    fn a_block_over_several_lines_is_offered_for_folding() {
        let h = highlighter("fn a() {\n  1\n}\nlet b = 2;");
        let folds = h.fold_ranges(&Rope::from("fn a() {\n  1\n}\nlet b = 2;"));
        assert_eq!(folds.len(), 1);
    }
}
