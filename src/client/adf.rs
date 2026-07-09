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
    ensure_inline(stack);
    let mut node = json!({ "type": "text", "text": text });
    if !marks.is_empty() {
        node["marks"] = json!(marks);
    }
    if let Some(f) = stack.last_mut() {
        f.children.push(node);
    }
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
