//! A declaration scanner for ordinary C declarators, without preprocessing.
//! Token offsets are UTF-8 byte offsets; untouched text is copied verbatim.

use crate::Result;
use regex::Regex;
use std::collections::BTreeSet;
use std::sync::LazyLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Punctuation,
    Identifier,
    Literal,
    Directive,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub text: String,
    pub start: usize,
    pub end: usize,
    pub kind: Kind,
}

fn character(text: &str, offset: usize) -> Option<char> {
    text.get(offset..)?.chars().next()
}

fn advance(text: &str, offset: usize) -> usize {
    offset + character(text, offset).map_or(0, char::len_utf8)
}

pub fn lex(text: &str, directives: bool) -> Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut i = 0;

    while let Some(ch) = character(text, i) {
        if ch.is_whitespace() {
            i += ch.len_utf8();
            continue;
        }

        let start = i;
        if text[i..].starts_with("//") {
            i = text[i..].find('\n').map_or(text.len(), |end| i + end);
            continue;
        }

        if text[i..].starts_with("/*") {
            i = text[i + 2..]
                .find("*/")
                .map(|end| i + 2 + end + 2)
                .ok_or("Unterminated C comment")?;
            continue;
        }

        let kind;
        if directives && ch == '#' {
            loop {
                let Some(end) = text[i..].find('\n').map(|end| i + end) else {
                    i = text.len();
                    break;
                };

                let last = if end > 0 && text.as_bytes()[end - 1] == b'\r' {
                    end.saturating_sub(2)
                } else {
                    end.saturating_sub(1)
                };
                i = end + 1;
                if end == 0 || text.as_bytes()[last] != b'\\' {
                    break;
                }
            }
            kind = Kind::Directive;
        } else {
            let mut quote = i;
            if matches!(ch, 'L' | 'u' | 'U') {
                quote += 1;
                if ch == 'u' && character(text, quote) == Some('8') {
                    quote += 1;
                }
            }

            if let Some(delimiter @ ('"' | '\'')) = character(text, quote) {
                i = quote + 1;
                while let Some(current) = character(text, i) {
                    if current == delimiter {
                        break;
                    }
                    if current == '\\' {
                        i = advance(text, i);
                    }
                    i = advance(text, i);
                }

                if i >= text.len() {
                    return Err("Unterminated C literal".into());
                }
                i += 1;
                kind = Kind::Literal;
            } else if ch.is_alphabetic() || ch == '_' {
                i += ch.len_utf8();
                while let Some(current) = character(text, i) {
                    if !current.is_alphanumeric() && current != '_' {
                        break;
                    }
                    i += current.len_utf8();
                }
                kind = Kind::Identifier;
            } else if ch.is_ascii_digit() {
                i += 1;
                while let Some(current) = character(text, i) {
                    if !current.is_alphanumeric() && !matches!(current, '_' | '.') {
                        break;
                    }
                    i += current.len_utf8();
                }
                kind = Kind::Literal;
            } else {
                i += ch.len_utf8();
                if ch == '-' && character(text, i) == Some('>') {
                    i += 1;
                }
                kind = Kind::Punctuation;
            }
        }

        tokens.push(Token {
            text: text[start..i].to_owned(),
            start,
            end: i,
            kind,
        });
    }

    Ok(tokens)
}

pub fn directives(text: &str) -> Result<Vec<Token>> {
    Ok(lex(text, true)?
        .into_iter()
        .filter(|t| t.kind == Kind::Directive)
        .collect())
}

fn builtin(s: &str) -> bool {
    matches!(
        s,
        "signed"
            | "unsigned"
            | "short"
            | "long"
            | "void"
            | "char"
            | "int"
            | "float"
            | "double"
            | "_Bool"
            | "_Complex"
    )
}

fn qualifier(s: &str) -> bool {
    matches!(s, "const" | "volatile" | "restrict")
}

fn specifier(s: &str) -> bool {
    builtin(s)
        || qualifier(s)
        || matches!(
            s,
            "typedef"
                | "static"
                | "extern"
                | "auto"
                | "register"
                | "inline"
                | "_Noreturn"
                | "_Thread_local"
        )
}

pub struct Scanner {
    tokens: Vec<Token>,
    names: BTreeSet<String>,
    tags: BTreeSet<String>,
    members: BTreeSet<usize>,
}

impl Scanner {
    fn at(&self, i: usize) -> &str {
        self.tokens.get(i).map_or("", |t| t.text.as_str())
    }

    fn is_id(&self, i: usize) -> bool {
        self.tokens
            .get(i)
            .is_some_and(|t| t.kind == Kind::Identifier)
    }

    fn error(&self, i: usize) -> String {
        format!(
            "Unsupported C declaration near '{}'. Use ordinary C declarators (not macro-generated declarations).",
            self.at(i)
        )
    }

    fn group(&self, start: usize) -> Result<usize> {
        let open = self.at(start);
        let close = match open {
            "(" => ")",
            "[" => "]",
            "{" => "}",
            _ => return Err(self.error(start)),
        };

        let mut depth = 1;
        for i in start + 1..self.tokens.len() {
            if self.at(i) == open {
                depth += 1;
            } else if self.at(i) == close {
                depth -= 1;
                if depth == 0 {
                    return Ok(i + 1);
                }
            }
        }
        Err(self.error(start))
    }

    fn declarator(&self, i: &mut usize) -> Result<usize> {
        while self.at(*i) == "*" {
            *i += 1;
            while qualifier(self.at(*i)) {
                *i += 1;
            }
        }

        let name;
        if self.at(*i) == "(" {
            *i += 1;
            name = self.declarator(i)?;
            if self.at(*i) != ")" {
                return Err(self.error(*i));
            }
            *i += 1;
        } else if self.is_id(*i) {
            name = *i;
            *i += 1;
        } else {
            return Err(self.error(*i));
        }

        while matches!(self.at(*i), "(" | "[") {
            *i = self.group(*i)?;
        }

        Ok(name)
    }

    fn enum_names(&mut self, start: usize, end: usize) -> Result<()> {
        let mut i = start;
        while i < end {
            if !self.is_id(i) {
                return Err(self.error(i));
            }

            self.names.insert(self.at(i).to_owned());
            i += 1;

            while i < end && self.at(i) != "," {
                i = if matches!(self.at(i), "(" | "[" | "{") {
                    self.group(i)?
                } else {
                    i + 1
                };
            }

            if self.at(i) == "," {
                i += 1;
            }
        }

        Ok(())
    }

    fn declaration(&mut self, start: usize, field: bool) -> Result<usize> {
        let mut i = start;
        let mut local = false;
        let mut has_type = false;

        while i < self.tokens.len() {
            let s = self.at(i).to_owned();
            if specifier(&s) {
                local |= matches!(s.as_str(), "static" | "typedef");
                has_type |= builtin(&s);
                i += 1;
                continue;
            }

            if matches!(s.as_str(), "_Alignas" | "_Atomic") {
                i += 1;
                if self.at(i) == "(" {
                    i = self.group(i)?;
                }
                has_type = true;
                continue;
            }

            if matches!(s.as_str(), "struct" | "union" | "enum") {
                i += 1;
                let tag = if self.is_id(i) {
                    let tag = self.at(i).to_owned();
                    i += 1;
                    Some(tag)
                } else {
                    None
                };

                if self.at(i) == "{" {
                    let end = self.group(i)?;
                    if !field && let Some(tag) = tag {
                        self.tags.insert(tag);
                    }
                    if s == "enum" && !field {
                        self.enum_names(i + 1, end - 1)?;
                    }
                    i = end;
                }
                has_type = true;
                continue;
            }

            if !has_type && self.is_id(i) {
                has_type = true;
                i += 1;
                continue;
            }
            break;
        }

        if self.at(i) == ";" {
            return Ok(i + 1);
        }

        loop {
            if !(field && self.at(i) == ":") {
                let name = self.declarator(&mut i)?;
                if field {
                    self.members.insert(self.tokens[name].start);
                } else if local {
                    self.names.insert(self.at(name).to_owned());
                }
            }

            if self.at(i) == "{" {
                return self.group(i);
            }

            if self.at(i) == "=" || (field && self.at(i) == ":") {
                i += 1;

                while i < self.tokens.len() && !matches!(self.at(i), "," | ";") {
                    i = if matches!(self.at(i), "(" | "[" | "{") {
                        self.group(i)?
                    } else {
                        i + 1
                    };
                }
            }

            if self.at(i) == ";" {
                return Ok(i + 1);
            }

            if self.at(i) != "," {
                return Err(self.error(i));
            }
            i += 1;
            if i >= self.tokens.len() {
                return Err(self.error(i));
            }
        }
    }

    pub fn new(text: &str) -> Result<Self> {
        static MACRO: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"^#\s*define\s+([A-Za-z_]\w*)").unwrap());

        let tokens = lex(text, true)?;
        let names = tokens
            .iter()
            .filter(|t| t.kind == Kind::Directive)
            .filter_map(|t| MACRO.captures(&t.text))
            .map(|m| m[1].to_owned())
            .collect();

        let mut scanner = Self {
            tokens: tokens
                .into_iter()
                .filter(|t| t.kind != Kind::Directive)
                .collect(),
            names,
            tags: BTreeSet::new(),
            members: BTreeSet::new(),
        };

        let mut i = 0;
        while i < scanner.tokens.len() {
            if scanner.at(i) == ";" {
                i += 1;
            } else if scanner.at(i) == "_Static_assert" {
                i = scanner.group(i + 1)?;
                if scanner.at(i) == ";" {
                    i += 1;
                }
            } else {
                i = scanner.declaration(i, false)?;
            }
        }

        // Member declarations have a separate namespace, even inside functions.
        for i in 0..scanner.tokens.len() {
            if !matches!(scanner.at(i), "struct" | "union") {
                continue;
            }

            let mut body = i + 1;
            if scanner.is_id(body) {
                body += 1;
            }
            if scanner.at(body) != "{" {
                continue;
            }

            let end = scanner.group(body)? - 1;
            let mut field = body + 1;
            while field < end {
                field = scanner.declaration(field, true)?;
            }
        }

        Ok(scanner)
    }

    pub fn private_names(&self) -> Vec<&str> {
        self.names.union(&self.tags).map(String::as_str).collect()
    }

    pub fn rewrite(&self, text: &str, prefix: &str) -> Result<String> {
        self.rewrite_inner(text, prefix, false)
    }

    fn rewrite_inner(&self, text: &str, prefix: &str, directive: bool) -> Result<String> {
        let mut result = String::new();
        let mut cursor = 0;
        let mut previous = String::new();

        for token in lex(text, !directive)? {
            result.push_str(&text[cursor..token.start]);

            if token.kind == Kind::Directive {
                result.push_str(&self.rewrite_inner(&token.text, prefix, true)?);
            } else {
                let is_tag = matches!(previous.as_str(), "struct" | "union" | "enum");
                let member = matches!(previous.as_str(), "." | "->")
                    || (!directive && self.members.contains(&token.start));
                let names = if is_tag { &self.tags } else { &self.names };
                if token.kind == Kind::Identifier && !member && names.contains(&token.text) {
                    result.push_str(prefix);
                }
                result.push_str(&token.text);
            }

            cursor = token.end;
            previous = token.text;
        }

        result.push_str(&text[cursor..]);

        Ok(result)
    }
}
