use serde_json::{Map, Value};

use crate::common::error::{Error, Result};

#[derive(Debug)]
struct Node {
    start: usize,
    end: usize,
    kind: NodeKind,
}

#[derive(Debug)]
enum NodeKind {
    Object(Vec<Member>),
    Array(Vec<Node>),
    Value,
}

#[derive(Debug)]
struct Member {
    key: String,
    value: Node,
}

struct Parser<'a> {
    text: &'a [u8],
    at: usize,
}

/// Read one key path from a JSON or JSONC document.
pub fn read_path(text: &str, path: &[&str]) -> Result<Option<Value>> {
    let root = parse(text)?;
    Ok(find(&root, path).and_then(|node| value_of(text.as_bytes(), Some(node))))
}

/// Set one key path while leaving bytes outside its parent object unchanged.
pub fn set_path(text: &str, path: &[&str], value: Value) -> Result<String> {
    if path.is_empty() {
        return Err(Error::InvalidParams("JSON path is empty".to_string()));
    }
    let root = parse(text)?;
    let mut document = value_of(text.as_bytes(), Some(&root))
        .ok_or_else(|| Error::InvalidParams("JSON root is missing".to_string()))?;
    set_value(&mut document, path, value)?;
    let parent_path = &path[..path.len() - 1];
    let parent = find(&root, parent_path);
    Ok(match parent {
        Some(node) => replace_node(text, node, parent_value(&document, parent_path)?),
        None => render_document(text, &document),
    })
}

/// Remove one key path while preserving the rest of the document.
pub fn remove_path(text: &str, path: &[&str]) -> Result<String> {
    if path.is_empty() {
        return Err(Error::InvalidParams("JSON path is empty".to_string()));
    }
    let root = parse(text)?;
    let Some(parent_node) = find(&root, &path[..path.len() - 1]) else {
        return Ok(text.to_string());
    };
    let mut document = value_of(text.as_bytes(), Some(&root))
        .ok_or_else(|| Error::InvalidParams("JSON root is missing".to_string()))?;
    if !remove_value(&mut document, path) {
        return Ok(text.to_string());
    }
    let parent_path = &path[..path.len() - 1];
    let parent_value = parent_value(&document, parent_path)?;
    Ok(replace_node(text, parent_node, parent_value))
}

fn parse(text: &str) -> Result<Node> {
    let mut parser = Parser {
        text: text.as_bytes(),
        at: if text.starts_with('\u{feff}') {
            '\u{feff}'.len_utf8()
        } else {
            0
        },
    };
    parser.trivia();
    let node = parser.value()?;
    parser.trivia();
    if parser.at != parser.text.len() {
        return Err(parser.error("trailing content"));
    }
    Ok(node)
}

impl Parser<'_> {
    fn error(&self, message: &str) -> Error {
        Error::InvalidParams(format!("invalid JSON/JSONC at byte {}: {message}", self.at))
    }

    fn trivia(&mut self) {
        loop {
            while self.at < self.text.len() && self.text[self.at].is_ascii_whitespace() {
                self.at += 1;
            }
            if self.text.get(self.at..self.at + 2) == Some(b"//") {
                self.at += 2;
                while self.at < self.text.len() && self.text[self.at] != b'\n' {
                    self.at += 1;
                }
                continue;
            }
            if self.text.get(self.at..self.at + 2) == Some(b"/*") {
                self.at += 2;
                while self.at + 1 < self.text.len()
                    && self.text.get(self.at..self.at + 2) != Some(b"*/")
                {
                    self.at += 1;
                }
                self.at = (self.at + 2).min(self.text.len());
                continue;
            }
            break;
        }
    }

    fn value(&mut self) -> Result<Node> {
        self.trivia();
        let start = self.at;
        match self.text.get(self.at) {
            Some(b'{') => self.object(start),
            Some(b'[') => self.array(start),
            Some(b'"') => {
                self.string()?;
                Ok(Node {
                    start,
                    end: self.at,
                    kind: NodeKind::Value,
                })
            }
            Some(_) => self.primitive(start),
            None => Err(self.error("expected a value")),
        }
    }

    fn object(&mut self, start: usize) -> Result<Node> {
        self.at += 1;
        let mut members: Vec<Member> = Vec::new();
        loop {
            self.trivia();
            if self.text.get(self.at) == Some(&b'}') {
                self.at += 1;
                return Ok(Node {
                    start,
                    end: self.at,
                    kind: NodeKind::Object(members),
                });
            }
            let key = self.string()?;
            self.trivia();
            self.expect(b':')?;
            let value = self.value()?;
            members.push(Member { key, value });
            self.trivia();
            match self.text.get(self.at) {
                Some(b',') => self.at += 1,
                Some(b'}') => {}
                _ => return Err(self.error("expected ',' or '}'")),
            }
        }
    }

    fn array(&mut self, start: usize) -> Result<Node> {
        self.at += 1;
        let mut values = Vec::new();
        loop {
            self.trivia();
            if self.text.get(self.at) == Some(&b']') {
                self.at += 1;
                return Ok(Node {
                    start,
                    end: self.at,
                    kind: NodeKind::Array(values),
                });
            }
            values.push(self.value()?);
            self.trivia();
            match self.text.get(self.at) {
                Some(b',') => self.at += 1,
                Some(b']') => {}
                _ => return Err(self.error("expected ',' or ']'")),
            }
        }
    }

    fn string(&mut self) -> Result<String> {
        if self.text.get(self.at) != Some(&b'"') {
            return Err(self.error("expected a string"));
        }
        let start = self.at;
        self.at += 1;
        while self.at < self.text.len() {
            match self.text[self.at] {
                b'\\' => self.at += 2,
                b'"' => {
                    self.at += 1;
                    return serde_json::from_slice(&self.text[start..self.at]).map_err(|error| {
                        Error::InvalidParams(format!("invalid JSON string: {error}"))
                    });
                }
                _ => self.at += 1,
            }
        }
        Err(self.error("unterminated string"))
    }

    fn primitive(&mut self, start: usize) -> Result<Node> {
        while self.at < self.text.len() {
            let byte = self.text[self.at];
            if byte.is_ascii_whitespace() || matches!(byte, b',' | b']' | b'}') {
                break;
            }
            self.at += 1;
        }
        if self.at == start {
            return Err(self.error("expected a primitive value"));
        }
        Ok(Node {
            start,
            end: self.at,
            kind: NodeKind::Value,
        })
    }

    fn expect(&mut self, byte: u8) -> Result<()> {
        if self.text.get(self.at) != Some(&byte) {
            return Err(self.error(&format!("expected '{}'", byte as char)));
        }
        self.at += 1;
        Ok(())
    }
}

fn find<'a>(node: &'a Node, path: &[&str]) -> Option<&'a Node> {
    let Some((key, rest)) = path.split_first() else {
        return Some(node);
    };
    let NodeKind::Object(members) = &node.kind else {
        return None;
    };
    members
        .iter()
        .find(|member| member.key == *key)
        .and_then(|member| find(&member.value, rest))
}

fn value_of(text: &[u8], node: Option<&Node>) -> Option<Value> {
    let node = node?;
    Some(match &node.kind {
        NodeKind::Object(members) => Value::Object(
            members
                .iter()
                .map(|member| {
                    (
                        member.key.clone(),
                        value_of(text, Some(&member.value)).unwrap_or(Value::Null),
                    )
                })
                .collect::<Map<_, _>>(),
        ),
        NodeKind::Array(values) => Value::Array(
            values
                .iter()
                .filter_map(|value| value_of(text, Some(value)))
                .collect(),
        ),
        NodeKind::Value => serde_json::from_slice(&text[node.start..node.end]).ok()?,
    })
}

fn set_value(document: &mut Value, path: &[&str], value: Value) -> Result<()> {
    let Some((key, rest)) = path.split_first() else {
        *document = value;
        return Ok(());
    };
    let object = document
        .as_object_mut()
        .ok_or_else(|| Error::InvalidParams(format!("{} is not an object", key)))?;
    if rest.is_empty() {
        object.insert((*key).to_string(), value);
        return Ok(());
    }
    let child = object
        .entry((*key).to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    set_value(child, rest, value)
}

fn remove_value(document: &mut Value, path: &[&str]) -> bool {
    let (Some((key, rest)), Some(object)) = (path.split_first(), document.as_object_mut()) else {
        return false;
    };
    if rest.is_empty() {
        return object.remove(*key).is_some();
    }
    object
        .get_mut(*key)
        .is_some_and(|child| remove_value(child, rest))
}

fn parent_value<'a>(document: &'a Value, path: &[&str]) -> Result<&'a Value> {
    path.iter().try_fold(document, |value, key| {
        value
            .get(*key)
            .ok_or_else(|| Error::InvalidParams(format!("missing JSON path {key}")))
    })
}

fn replace_node(text: &str, node: &Node, value: &Value) -> String {
    let rendered = render_value(value, line_indent(text, node.start), line_ending(text));
    let mut out = String::with_capacity(text.len() + rendered.len());
    out.push_str(&text[..node.start]);
    out.push_str(&rendered);
    out.push_str(&text[node.end..]);
    out
}

fn render_document(text: &str, value: &Value) -> String {
    let mut rendered = render_value(value, "", line_ending(text));
    rendered.push_str(line_ending(text));
    rendered
}

fn render_value(value: &Value, base: &str, newline: &str) -> String {
    let body = serde_json::to_string_pretty(value).unwrap_or_else(|_| "null".to_string());
    let mut out = String::with_capacity(body.len());
    for (index, line) in body.lines().enumerate() {
        if index > 0 {
            out.push_str(newline);
        }
        out.push_str(base);
        out.push_str(line);
    }
    out
}

fn line_indent(text: &str, at: usize) -> &str {
    let start = text[..at].rfind('\n').map_or(0, |index| index + 1);
    let prefix = &text[start..at];
    let width = prefix.len() - prefix.trim_start().len();
    &prefix[..width]
}

fn line_ending(text: &str) -> &'static str {
    if text.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}
