//! Thin Markdown/UTF-16 adapter over shared original-byte text boundaries.
//! Caret flooring here never relaxes strict edit or ghost coordinate conversion.
use markdown::mdast::Node;
use markdown::{Constructs, ParseOptions};
use serde_json::{json, Value};
use terraphim_alternatives::byte_to_utf16;
use terraphim_alternatives::text::{self, ByteRange, TextUnit};
use wasm_bindgen::prelude::*;

/// A targeting refusal, with no document mutation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TextTargetError {
    #[error("Unknown text kind: {0}; use word, sentence or paragraph")]
    UnknownKind(String),
    #[error("Caret offset is outside the current text")]
    OffsetOutOfRange,
    #[error("Markdown structure could not be parsed")]
    MarkdownParse,
    #[error("Text range is not on original UTF-8 scalar boundaries")]
    InvalidBoundary,
}

struct Region {
    outer: ByteRange,
    prose: ByteRange,
    heading: bool,
    protected: Vec<ByteRange>,
}

fn span(node: &Node) -> Option<ByteRange> {
    node.position().map(|p| ByteRange {
        start: p.start.offset,
        end: p.end.offset,
    })
}

// mdast owns all Markdown syntax. Inline code and non-visible link destinations
// cannot contribute sentence boundaries; visible link text remains prose.
fn inline_protection(node: &Node, text: &str, protected: &mut Vec<ByteRange>) {
    match node {
        Node::InlineCode(_)
        | Node::Html(_)
        | Node::Image(_)
        | Node::ImageReference(_)
        | Node::InlineMath(_)
        | Node::FootnoteReference(_) => {
            if let Some(range) = span(node) {
                protected.push(range);
            }
            return;
        }
        Node::Link(link) => {
            if let Some(range) = span(node) {
                let child_end = link.children.last().and_then(span).map(|r| r.end);
                if text[range.start..range.end].starts_with('[') {
                    if let Some(end) = child_end {
                        protected.push(ByteRange {
                            start: range.start,
                            end: range.start + 1,
                        });
                        protected.push(ByteRange {
                            start: end,
                            end: range.end,
                        });
                    }
                } else {
                    protected.push(range);
                    return;
                }
            }
        }
        Node::LinkReference(link) => {
            if let Some(range) = span(node) {
                if let Some(end) = link.children.last().and_then(span).map(|r| r.end) {
                    protected.push(ByteRange {
                        start: range.start,
                        end: range.start + 1,
                    });
                    protected.push(ByteRange {
                        start: end,
                        end: range.end,
                    });
                }
            }
        }
        _ => {}
    }
    if let Some(children) = node.children() {
        for child in children {
            inline_protection(child, text, protected);
        }
    }
}

fn collect_regions(node: &Node, text: &str, regions: &mut Vec<Region>) {
    match node {
        Node::Code(_)
        | Node::Html(_)
        | Node::Table(_)
        | Node::Blockquote(_)
        | Node::Definition(_)
        | Node::ThematicBreak(_)
        | Node::Yaml(_)
        | Node::Toml(_)
        | Node::Math(_)
        | Node::FootnoteDefinition(_) => return,
        Node::Paragraph(_) | Node::Heading(_) => {
            if let Some(outer) = span(node) {
                let heading = matches!(node, Node::Heading(_));
                let prose = if heading {
                    let Some(children) = node.children() else {
                        return;
                    };
                    let Some(first) = children.first().and_then(span) else {
                        return;
                    };
                    let Some(last) = children.last().and_then(span) else {
                        return;
                    };
                    ByteRange {
                        start: first.start,
                        end: last.end,
                    }
                } else {
                    outer
                };
                let mut protected = Vec::new();
                inline_protection(node, text, &mut protected);
                regions.push(Region {
                    outer,
                    prose,
                    heading,
                    protected,
                });
            }
            return;
        }
        _ => {}
    }
    if let Some(children) = node.children() {
        for child in children {
            collect_regions(child, text, regions);
        }
    }
}

fn caret_byte(text: &str, utf16: usize) -> Result<usize, TextTargetError> {
    let mut units = 0;
    for (byte, c) in text.char_indices() {
        let next = units + c.len_utf16();
        if utf16 < next {
            return Ok(byte);
        }
        units = next;
    }
    if utf16 == units {
        Ok(text.len())
    } else {
        Err(TextTargetError::OffsetOutOfRange)
    }
}

/// Pure adapter used by the WASM binding. Returns null when a valid caret has
/// no prose target, otherwise `{start,end}` in UTF-16. Unknown kinds and offsets
/// beyond the text are typed errors; half-surrogate carets floor to their scalar.
pub fn text_range_json(text: &str, utf16: u32, kind: &str) -> Result<Value, TextTargetError> {
    let unit = match kind {
        "word" => TextUnit::Word,
        "sentence" => TextUnit::Sentence,
        "paragraph" => TextUnit::Paragraph,
        _ => return Err(TextTargetError::UnknownKind(kind.into())),
    };
    let byte = caret_byte(text, utf16 as usize)?;
    let options = ParseOptions {
        constructs: Constructs {
            frontmatter: true,
            ..Constructs::gfm()
        },
        ..ParseOptions::gfm()
    };
    let root = markdown::to_mdast(text, &options).map_err(|_| TextTargetError::MarkdownParse)?;
    let mut regions = Vec::new();
    collect_regions(&root, text, &mut regions);
    let Some(region) = regions
        .iter()
        .find(|r| r.outer.start <= byte && byte <= r.outer.end)
    else {
        return Ok(Value::Null);
    };
    let range = if region.heading && unit != TextUnit::Word {
        Some(region.prose)
    } else {
        let prose = &text[region.prose.start..region.prose.end];
        if byte < region.prose.start || byte > region.prose.end {
            return Ok(Value::Null);
        }
        let local = byte - region.prose.start;
        match unit {
            TextUnit::Word => text::range_at(prose, local, TextUnit::Word).map(|r| ByteRange {
                start: r.start + region.prose.start,
                end: r.end + region.prose.start,
            }),
            TextUnit::Sentence => {
                let protected: Vec<_> = region
                    .protected
                    .iter()
                    .map(|r| ByteRange {
                        start: r.start - region.prose.start,
                        end: r.end - region.prose.start,
                    })
                    .collect();
                text::sentence_ranges_with_protected(prose, &protected)
                    .into_iter()
                    .find(|r| r.start <= local && local <= r.end)
                    .map(|r| ByteRange {
                        start: r.start + region.prose.start,
                        end: r.end + region.prose.start,
                    })
            }
            TextUnit::Paragraph => {
                text::range_at(prose, local, TextUnit::Paragraph).map(|r| ByteRange {
                    start: r.start + region.prose.start,
                    end: r.end + region.prose.start,
                })
            }
        }
    };
    let Some(range) = range else {
        return Ok(Value::Null);
    };
    let start = byte_to_utf16(text, range.start).ok_or(TextTargetError::InvalidBoundary)?;
    let end = byte_to_utf16(text, range.end).ok_or(TextTargetError::InvalidBoundary)?;
    Ok(json!({"start":start,"end":end}))
}

/// Shared automatic targeting; coordinates and return object are UTF-16.
#[wasm_bindgen]
pub fn text_range_at(text: &str, utf16: u32, kind: &str) -> Result<JsValue, JsValue> {
    let value =
        text_range_json(text, utf16, kind).map_err(|e| JsValue::from_str(&e.to_string()))?;
    js_sys::JSON::parse(&value.to_string())
}

/// Explicit-selection kind inference through shared Rust boundaries/counter.
#[wasm_bindgen]
pub fn text_infer_kind(text: &str) -> String {
    match text::infer_kind(text) {
        TextUnit::Word => "word",
        TextUnit::Sentence => "sentence",
        TextUnit::Paragraph => "paragraph",
    }
    .into()
}
