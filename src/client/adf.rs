//! Convert Markdown text into Atlassian Document Format (ADF) JSON.
//!
//! Jira's REST API stores rich text as ADF, not Markdown. When a user writes a
//! comment or description in Markdown, we parse it and emit the equivalent ADF
//! document so headings, lists, bold/italic, code, links, etc. render natively
//! in Jira instead of appearing as literal Markdown source.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Parser, Tag, TagEnd};
use serde_json::{json, Value};

/// Convert a Markdown string into an ADF `doc` node.
pub fn markdown_to_adf(md: &str) -> Value {
    let mut content = build_blocks(md);
    if content.is_empty() {
        content.push(json!({ "type": "paragraph", "content": [] }));
    }
    json!({
        "type": "doc",
        "version": 1,
        "content": content,
    })
}

#[derive(Debug)]
enum Kind {
    Root,
    Paragraph,
    Heading(u8),
    BulletList,
    OrderedList(u64),
    ListItem,
    Blockquote,
    CodeBlock(Option<String>),
}

struct Frame {
    kind: Kind,
    children: Vec<Value>,
    implicit: bool,
}

impl Frame {
    fn new(kind: Kind) -> Self {
        Frame { kind, children: Vec::new(), implicit: false }
    }
    /// A frame whose ADF content must be block nodes (not inline text).
    fn expects_blocks(&self) -> bool {
        matches!(
            self.kind,
            Kind::Root | Kind::ListItem | Kind::Blockquote | Kind::BulletList | Kind::OrderedList(_)
        )
    }
}

fn build_blocks(md: &str) -> Vec<Value> {
    let parser = Parser::new(md);
    let mut stack: Vec<Frame> = vec![Frame::new(Kind::Root)];
    let mut marks: Vec<Value> = Vec::new();

    for event in parser {
        match event {
            Event::Start(tag) => {
                close_implicit_para(&mut stack);
                match tag {
                    Tag::Paragraph => stack.push(Frame::new(Kind::Paragraph)),
                    Tag::Heading { level, .. } => {
                        stack.push(Frame::new(Kind::Heading(heading_level(level))))
                    }
                    Tag::List(Some(start)) => {
                        stack.push(Frame::new(Kind::OrderedList(start)))
                    }
                    Tag::List(None) => stack.push(Frame::new(Kind::BulletList)),
                    Tag::Item => stack.push(Frame::new(Kind::ListItem)),
                    Tag::BlockQuote(_) => stack.push(Frame::new(Kind::Blockquote)),
                    Tag::CodeBlock(kind) => {
                        let lang = match kind {
                            CodeBlockKind::Fenced(l) if !l.is_empty() => Some(l.to_string()),
                            _ => None,
                        };
                        stack.push(Frame::new(Kind::CodeBlock(lang)));
                    }
                    Tag::Emphasis => marks.push(json!({ "type": "em" })),
                    Tag::Strong => marks.push(json!({ "type": "strong" })),
                    Tag::Strikethrough => marks.push(json!({ "type": "strike" })),
                    Tag::Link { dest_url, .. } => {
                        marks.push(json!({ "type": "link", "attrs": { "href": dest_url.to_string() } }))
                    }
                    _ => {}
                }
            }
            Event::End(tag) => match tag {
                TagEnd::Emphasis
                | TagEnd::Strong
                | TagEnd::Strikethrough
                | TagEnd::Link => {
                    marks.pop();
                }
                TagEnd::Item | TagEnd::BlockQuote(_) | TagEnd::List(_) => {
                    close_implicit_para(&mut stack);
                    close_frame(&mut stack);
                }
                _ => close_frame(&mut stack),
            },
            Event::Text(t) => {
                if in_code_block(&stack) {
                    push_raw_text(&mut stack, &t);
                } else {
                    push_text(&mut stack, &marks, &t);
                }
            }
            Event::Code(t) => {
                let mut m = marks.clone();
                m.push(json!({ "type": "code" }));
                push_text(&mut stack, &m, &t);
            }
            Event::SoftBreak => {
                if in_code_block(&stack) {
                    push_raw_text(&mut stack, "\n");
                } else {
                    push_text(&mut stack, &marks, " ");
                }
            }
            Event::HardBreak => push_hard_break(&mut stack),
            _ => {}
        }
    }

    stack.pop().map(|f| f.children).unwrap_or_default()
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn in_code_block(stack: &[Frame]) -> bool {
    matches!(stack.last().map(|f| &f.kind), Some(Kind::CodeBlock(_)))
}

/// Ensure there is an inline-accepting frame on top; open an implicit paragraph
/// if the current frame expects block content (e.g. a tight list item).
fn ensure_inline(stack: &mut Vec<Frame>) {
    if stack.last().map(|f| f.expects_blocks()).unwrap_or(false) {
        let mut f = Frame::new(Kind::Paragraph);
        f.implicit = true;
        stack.push(f);
    }
}

fn close_implicit_para(stack: &mut Vec<Frame>) {
    if let Some(f) = stack.last() {
        if f.implicit && matches!(f.kind, Kind::Paragraph) {
            close_frame(stack);
        }
    }
}

fn push_text(stack: &mut Vec<Frame>, marks: &[Value], text: &str) {
    if text.is_empty() {
        return;
    }
    // Don't autolink when the text is already inside a link (avoids nesting) or
    // an inline code span (a URL in `code` should stay literal).
    let already_linked = marks.iter().any(|m| {
        matches!(
            m.get("type").and_then(Value::as_str),
            Some("link") | Some("code")
        )
    });
    if already_linked {
        emit_text_node(stack, marks, text);
        return;
    }
    for (segment, is_url) in split_urls(text) {
        if is_url {
            let mut m = marks.to_vec();
            m.push(json!({ "type": "link", "attrs": { "href": segment } }));
            emit_text_node(stack, &m, &segment);
        } else {
            emit_text_node(stack, marks, &segment);
        }
    }
}

fn emit_text_node(stack: &mut Vec<Frame>, marks: &[Value], text: &str) {
    if text.is_empty() {
        return;
    }
    ensure_inline(stack);
    let mut node = json!({ "type": "text", "text": text });
    if !marks.is_empty() {
        node["marks"] = json!(marks);
    }
    if let Some(f) = stack.last_mut() {
        f.children.push(node);
    }
}

/// Split text into segments, flagging bare `http(s)://` URLs so callers can
/// attach a link mark. pulldown-cmark does not autolink bare URLs, so we do it.
fn split_urls(text: &str) -> Vec<(String, bool)> {
    let mut out: Vec<(String, bool)> = Vec::new();
    let mut rest = text;
    while let Some(start) = find_url_start(rest) {
        let (before, from) = rest.split_at(start);
        if !before.is_empty() {
            out.push((before.to_string(), false));
        }
        let end = from.find(char::is_whitespace).unwrap_or(from.len());
        let (candidate, tail) = from.split_at(end);
        // Strip trailing punctuation that is almost never part of a URL.
        let core = candidate
            .trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']', '}', '"', '\'']);
        let (url, trailing) = candidate.split_at(core.len());
        out.push((url.to_string(), true));
        if !trailing.is_empty() {
            out.push((trailing.to_string(), false));
        }
        rest = tail;
    }
    if !rest.is_empty() {
        out.push((rest.to_string(), false));
    }
    out
}

fn find_url_start(s: &str) -> Option<usize> {
    let mut i = 0;
    while let Some(p) = s[i..].find("http") {
        let idx = i + p;
        let rem = &s[idx..];
        if rem.starts_with("http://") || rem.starts_with("https://") {
            return Some(idx);
        }
        i = idx + 4;
    }
    None
}

fn push_raw_text(stack: &mut Vec<Frame>, text: &str) {
    if let Some(f) = stack.last_mut() {
        f.children.push(json!({ "type": "text", "text": text }));
    }
}

fn push_hard_break(stack: &mut Vec<Frame>) {
    ensure_inline(stack);
    if let Some(f) = stack.last_mut() {
        f.children.push(json!({ "type": "hardBreak" }));
    }
}

/// Pop the top frame, build its ADF node, and attach it to the parent.
fn close_frame(stack: &mut Vec<Frame>) {
    let frame = match stack.pop() {
        Some(f) => f,
        None => return,
    };
    let node = match frame.kind {
        Kind::Root => {
            // Should not happen; restore and bail.
            stack.push(frame);
            return;
        }
        Kind::Paragraph => json!({ "type": "paragraph", "content": frame.children }),
        Kind::Heading(level) => json!({
            "type": "heading",
            "attrs": { "level": level },
            "content": frame.children,
        }),
        Kind::BulletList => json!({ "type": "bulletList", "content": frame.children }),
        Kind::OrderedList(start) => json!({
            "type": "orderedList",
            "attrs": { "order": start },
            "content": frame.children,
        }),
        Kind::ListItem => {
            // ADF listItem must contain block content.
            let content = if frame.children.is_empty() {
                vec![json!({ "type": "paragraph", "content": [] })]
            } else {
                frame.children
            };
            json!({ "type": "listItem", "content": content })
        }
        Kind::Blockquote => json!({ "type": "blockquote", "content": frame.children }),
        Kind::CodeBlock(lang) => {
            let text: String = frame
                .children
                .iter()
                .filter_map(|c| c["text"].as_str())
                .collect();
            let text = text.strip_suffix('\n').unwrap_or(&text).to_string();
            let mut node = json!({ "type": "codeBlock", "content": [] });
            if let Some(l) = lang {
                node["attrs"] = json!({ "language": l });
            }
            if !text.is_empty() {
                node["content"] = json!([{ "type": "text", "text": text }]);
            }
            node
        }
    };
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(content: Value) -> Value {
        json!({ "type": "doc", "version": 1, "content": content })
    }

    #[test]
    fn plain_paragraph() {
        assert_eq!(
            markdown_to_adf("hello world"),
            doc(json!([{
                "type": "paragraph",
                "content": [{ "type": "text", "text": "hello world" }]
            }]))
        );
    }

    #[test]
    fn heading() {
        assert_eq!(
            markdown_to_adf("## Title"),
            doc(json!([{
                "type": "heading",
                "attrs": { "level": 2 },
                "content": [{ "type": "text", "text": "Title" }]
            }]))
        );
    }

    #[test]
    fn bold_and_italic() {
        let adf = markdown_to_adf("**bold** and *italic*");
        let content = &adf["content"][0]["content"];
        assert_eq!(content[0]["text"], "bold");
        assert_eq!(content[0]["marks"][0]["type"], "strong");
        assert_eq!(content[2]["text"], "italic");
        assert_eq!(content[2]["marks"][0]["type"], "em");
    }

    #[test]
    fn bullet_list() {
        let adf = markdown_to_adf("- one\n- two");
        let list = &adf["content"][0];
        assert_eq!(list["type"], "bulletList");
        assert_eq!(list["content"][0]["type"], "listItem");
        assert_eq!(
            list["content"][0]["content"][0]["content"][0]["text"],
            "one"
        );
        assert_eq!(
            list["content"][1]["content"][0]["content"][0]["text"],
            "two"
        );
    }

    #[test]
    fn ordered_list() {
        let adf = markdown_to_adf("1. first\n2. second");
        let list = &adf["content"][0];
        assert_eq!(list["type"], "orderedList");
        assert_eq!(list["attrs"]["order"], 1);
        assert_eq!(list["content"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn inline_code() {
        let adf = markdown_to_adf("run `cargo build` now");
        let content = &adf["content"][0]["content"];
        assert_eq!(content[1]["text"], "cargo build");
        assert_eq!(content[1]["marks"][0]["type"], "code");
    }

    #[test]
    fn code_block() {
        let adf = markdown_to_adf("```rust\nfn main() {}\n```");
        let block = &adf["content"][0];
        assert_eq!(block["type"], "codeBlock");
        assert_eq!(block["attrs"]["language"], "rust");
        assert_eq!(block["content"][0]["text"], "fn main() {}");
    }

    #[test]
    fn link() {
        let adf = markdown_to_adf("[text](https://example.com)");
        let node = &adf["content"][0]["content"][0];
        assert_eq!(node["text"], "text");
        assert_eq!(node["marks"][0]["type"], "link");
        assert_eq!(node["marks"][0]["attrs"]["href"], "https://example.com");
    }

    #[test]
    fn bare_url_becomes_link() {
        let adf = markdown_to_adf("see https://github.com/mistsys/mobius/compare/v1...v2 now");
        let content = &adf["content"][0]["content"];
        // "see " | url(link) | " now"
        assert_eq!(content[0]["text"], "see ");
        assert!(content[0].get("marks").is_none());
        assert_eq!(content[1]["text"], "https://github.com/mistsys/mobius/compare/v1...v2");
        assert_eq!(content[1]["marks"][0]["type"], "link");
        assert_eq!(
            content[1]["marks"][0]["attrs"]["href"],
            "https://github.com/mistsys/mobius/compare/v1...v2"
        );
        assert_eq!(content[2]["text"], " now");
    }

    #[test]
    fn bare_url_trailing_punctuation_excluded() {
        let adf = markdown_to_adf("visit https://example.com.");
        let content = &adf["content"][0]["content"];
        assert_eq!(content[1]["text"], "https://example.com");
        assert_eq!(content[1]["marks"][0]["attrs"]["href"], "https://example.com");
        assert_eq!(content[2]["text"], ".");
    }

    #[test]
    fn markdown_link_not_double_wrapped() {
        // A proper [text](url) link must keep display text and a single link mark.
        let adf = markdown_to_adf("[docs](https://example.com)");
        let node = &adf["content"][0]["content"][0];
        assert_eq!(node["text"], "docs");
        assert_eq!(node["marks"].as_array().unwrap().len(), 1);
        assert_eq!(node["marks"][0]["type"], "link");
    }

    #[test]
    fn url_in_code_span_stays_literal() {
        let adf = markdown_to_adf("`https://example.com`");
        let node = &adf["content"][0]["content"][0];
        assert_eq!(node["text"], "https://example.com");
        assert_eq!(node["marks"][0]["type"], "code");
        assert_eq!(node["marks"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn empty_input_yields_empty_paragraph() {
        assert_eq!(
            markdown_to_adf(""),
            doc(json!([{ "type": "paragraph", "content": [] }]))
        );
    }

    #[test]
    fn multi_paragraph() {
        let adf = markdown_to_adf("first\n\nsecond");
        assert_eq!(adf["content"].as_array().unwrap().len(), 2);
        assert_eq!(adf["content"][0]["content"][0]["text"], "first");
        assert_eq!(adf["content"][1]["content"][0]["text"], "second");
    }
}
