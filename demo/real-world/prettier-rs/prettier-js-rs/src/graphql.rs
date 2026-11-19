//! `src/language-graphql/` (parser-graphql.js, printer-graphql.js, print/description.js) with the
//! parts of graphql-js 16.11 (`language/lexer.mjs`, `parser.mjs`, `blockString.mjs`) it uses, and
//! Prettier's generic comment attach and print (`src/main/comments/`) without language handlers.
//! graphql-js is MIT licensed, Copyright (c) GraphQL Contributors. Offsets are byte offsets.

use crate::comments::is_js_whitespace;
use crate::doc::*;
use crate::options::Options;
use crate::printer::{R, bail};
use crate::text::{has_newline, is_next_line_empty, skip_newline, skip_spaces};

#[derive(Clone, Copy, PartialEq, Debug)]
enum Tk {
    Sof,
    Eof,
    Bang,
    Dollar,
    Amp,
    ParenL,
    ParenR,
    Spread,
    Colon,
    Equals,
    At,
    BracketL,
    BracketR,
    BraceL,
    Pipe,
    BraceR,
    Name,
    Int,
    Float,
    Str,
    BlockStr,
}

struct Token {
    kind: Tk,
    start: usize,
    end: usize,
    value: String,
}

struct Comment {
    start: usize,
    end: usize,
    value: String,
}

fn is_name_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_name_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn hex(b: Option<&u8>) -> i64 {
    match b {
        Some(c @ b'0'..=b'9') => i64::from(c - b'0'),
        Some(c @ b'A'..=b'F') => i64::from(c - b'A' + 10),
        Some(c @ b'a'..=b'f') => i64::from(c - b'a' + 10),
        _ => -1,
    }
}

fn is_scalar(code: i64) -> bool {
    (0..=0xd7ff).contains(&code) || (0xe000..=0x10ffff).contains(&code)
}

fn read_16(b: &[u8], p: usize) -> i64 {
    (hex(b.get(p)) << 12) | (hex(b.get(p + 1)) << 8) | (hex(b.get(p + 2)) << 4) | hex(b.get(p + 3))
}

fn lex(text: &str) -> Option<(Vec<Token>, Vec<Comment>)> {
    let b = text.as_bytes();
    let n = b.len();
    let mut tokens = vec![Token {
        kind: Tk::Sof,
        start: 0,
        end: 0,
        value: String::new(),
    }];
    let mut comments = Vec::new();
    let mut p = 0;
    let punct = |kind: Tk, start: usize, len: usize| Token {
        kind,
        start,
        end: start + len,
        value: String::new(),
    };
    while p < n {
        let c = b[p];
        match c {
            b'\t' | b' ' | b',' | b'\n' | b'\r' => {
                p += 1;
                continue;
            }
            0xEF if b.get(p + 1) == Some(&0xBB) && b.get(p + 2) == Some(&0xBF) => {
                p += 3;
                continue;
            }
            b'#' => {
                let mut e = p + 1;
                while e < n && b[e] != b'\n' && b[e] != b'\r' {
                    e += 1;
                }
                comments.push(Comment {
                    start: p,
                    end: e,
                    value: text[p + 1..e].to_string(),
                });
                p = e;
                continue;
            }
            b'!' => tokens.push(punct(Tk::Bang, p, 1)),
            b'$' => tokens.push(punct(Tk::Dollar, p, 1)),
            b'&' => tokens.push(punct(Tk::Amp, p, 1)),
            b'(' => tokens.push(punct(Tk::ParenL, p, 1)),
            b')' => tokens.push(punct(Tk::ParenR, p, 1)),
            b'.' => {
                if b.get(p + 1) == Some(&b'.') && b.get(p + 2) == Some(&b'.') {
                    tokens.push(punct(Tk::Spread, p, 3));
                } else {
                    return None;
                }
            }
            b':' => tokens.push(punct(Tk::Colon, p, 1)),
            b'=' => tokens.push(punct(Tk::Equals, p, 1)),
            b'@' => tokens.push(punct(Tk::At, p, 1)),
            b'[' => tokens.push(punct(Tk::BracketL, p, 1)),
            b']' => tokens.push(punct(Tk::BracketR, p, 1)),
            b'{' => tokens.push(punct(Tk::BraceL, p, 1)),
            b'|' => tokens.push(punct(Tk::Pipe, p, 1)),
            b'}' => tokens.push(punct(Tk::BraceR, p, 1)),
            b'"' => {
                let t = if b.get(p + 1) == Some(&b'"') && b.get(p + 2) == Some(&b'"') {
                    read_block_string(text, p)?
                } else {
                    read_string(text, p)?
                };
                p = t.end;
                tokens.push(t);
                continue;
            }
            c if c.is_ascii_digit() || c == b'-' => {
                let t = read_number(b, p)?;
                p = t.end;
                tokens.push(Token {
                    value: text[t.start..t.end].to_string(),
                    ..t
                });
                continue;
            }
            c if is_name_start(c) => {
                let mut e = p + 1;
                while e < n && is_name_continue(b[e]) {
                    e += 1;
                }
                tokens.push(Token {
                    kind: Tk::Name,
                    start: p,
                    end: e,
                    value: text[p..e].to_string(),
                });
                p = e;
                continue;
            }
            _ => return None,
        }
        p = tokens.last().map_or(p + 1, |t| t.end);
    }
    tokens.push(punct(Tk::Eof, n, 0));
    Some((tokens, comments))
}

fn read_digits(b: &[u8], p: usize) -> Option<usize> {
    if !b.get(p).is_some_and(u8::is_ascii_digit) {
        return None;
    }
    let mut p = p + 1;
    while b.get(p).is_some_and(u8::is_ascii_digit) {
        p += 1;
    }
    Some(p)
}

fn read_number(b: &[u8], start: usize) -> Option<Token> {
    let mut p = start;
    let mut is_float = false;
    if b[p] == b'-' {
        p += 1;
    }
    if b.get(p) == Some(&b'0') {
        p += 1;
        if b.get(p).is_some_and(u8::is_ascii_digit) {
            return None;
        }
    } else {
        p = read_digits(b, p)?;
    }
    if b.get(p) == Some(&b'.') {
        is_float = true;
        p = read_digits(b, p + 1)?;
    }
    if matches!(b.get(p), Some(b'e' | b'E')) {
        is_float = true;
        p += 1;
        if matches!(b.get(p), Some(b'+' | b'-')) {
            p += 1;
        }
        p = read_digits(b, p)?;
    }
    if b.get(p).is_some_and(|&c| c == b'.' || is_name_start(c)) {
        return None;
    }
    Some(Token {
        kind: if is_float { Tk::Float } else { Tk::Int },
        start,
        end: p,
        value: String::new(),
    })
}

fn read_string(text: &str, start: usize) -> Option<Token> {
    let b = text.as_bytes();
    let mut p = start + 1;
    let mut chunk = p;
    let mut value = String::new();
    while p < b.len() {
        match b[p] {
            b'"' => {
                value.push_str(&text[chunk..p]);
                return Some(Token {
                    kind: Tk::Str,
                    start,
                    end: p + 1,
                    value,
                });
            }
            b'\\' => {
                value.push_str(&text[chunk..p]);
                let (ch, size) = match b.get(p + 1) {
                    Some(b'u') if b.get(p + 2) == Some(&b'{') => {
                        let mut point: i64 = 0;
                        let mut size = 3;
                        let mut out = None;
                        while size < 12 {
                            let c = b.get(p + size);
                            size += 1;
                            if c == Some(&b'}') {
                                if size < 5 || !is_scalar(point) {
                                    break;
                                }
                                out = Some((char::from_u32(point as u32)?.to_string(), size));
                                break;
                            }
                            point = (point << 4) | hex(c);
                            if point < 0 {
                                break;
                            }
                        }
                        out?
                    }
                    Some(b'u') => {
                        let code = read_16(b, p + 2);
                        if is_scalar(code) {
                            (char::from_u32(code as u32)?.to_string(), 6)
                        } else if (0xd800..=0xdbff).contains(&code)
                            && b.get(p + 6) == Some(&b'\\')
                            && b.get(p + 7) == Some(&b'u')
                        {
                            let trail = read_16(b, p + 8);
                            if !(0xdc00..=0xdfff).contains(&trail) {
                                return None;
                            }
                            let point = 0x10000 + ((code - 0xd800) << 10) + (trail - 0xdc00);
                            (char::from_u32(point as u32)?.to_string(), 12)
                        } else {
                            return None;
                        }
                    }
                    Some(b'"') => ("\"".to_string(), 2),
                    Some(b'\\') => ("\\".to_string(), 2),
                    Some(b'/') => ("/".to_string(), 2),
                    Some(b'b') => ("\u{8}".to_string(), 2),
                    Some(b'f') => ("\u{c}".to_string(), 2),
                    Some(b'n') => ("\n".to_string(), 2),
                    Some(b'r') => ("\r".to_string(), 2),
                    Some(b't') => ("\t".to_string(), 2),
                    _ => return None,
                };
                value.push_str(&ch);
                p += size;
                chunk = p;
            }
            b'\n' | b'\r' => return None,
            _ => p += 1,
        }
    }
    None
}

fn leading_whitespace(s: &str) -> usize {
    s.bytes().take_while(|&c| c == b' ' || c == b'\t').count()
}

fn read_block_string(text: &str, start: usize) -> Option<Token> {
    let b = text.as_bytes();
    let mut p = start + 3;
    let mut chunk = p;
    let mut current = String::new();
    let mut lines: Vec<String> = Vec::new();
    while p < b.len() {
        if b[p] == b'"' && b.get(p + 1) == Some(&b'"') && b.get(p + 2) == Some(&b'"') {
            current.push_str(&text[chunk..p]);
            lines.push(current);
            let mut common = usize::MAX;
            let mut first = None;
            let mut last: isize = -1;
            for (i, line) in lines.iter().enumerate() {
                let ind = leading_whitespace(line);
                if ind == line.len() {
                    continue;
                }
                first.get_or_insert(i);
                last = i as isize;
                if i != 0 && ind < common {
                    common = ind;
                }
            }
            let dedented: Vec<&str> = lines
                .iter()
                .enumerate()
                .map(|(i, l)| {
                    if i == 0 {
                        l.as_str()
                    } else {
                        l.get(common..).unwrap_or("")
                    }
                })
                .collect();
            let from = first.unwrap_or(0);
            let to = (last + 1) as usize;
            let value = if from < to {
                dedented[from..to].join("\n")
            } else {
                String::new()
            };
            return Some(Token {
                kind: Tk::BlockStr,
                start,
                end: p + 3,
                value,
            });
        }
        if b[p] == b'\\'
            && b.get(p + 1) == Some(&b'"')
            && b.get(p + 2) == Some(&b'"')
            && b.get(p + 3) == Some(&b'"')
        {
            current.push_str(&text[chunk..p]);
            chunk = p + 1;
            p += 4;
            continue;
        }
        if b[p] == b'\n' || b[p] == b'\r' {
            current.push_str(&text[chunk..p]);
            lines.push(std::mem::take(&mut current));
            p += if b[p] == b'\r' && b.get(p + 1) == Some(&b'\n') {
                2
            } else {
                1
            };
            chunk = p;
            continue;
        }
        p += 1;
    }
    None
}

enum Child {
    One(Node),
    Many(Vec<Node>),
}

pub struct Node {
    kind: &'static str,
    start: usize,
    end: usize,
    id: usize,
    value: String,
    flag: bool,
    operation: &'static str,
    kids: Vec<(&'static str, Child)>,
}

impl Node {
    fn one(&self, key: &str) -> Option<&Node> {
        self.kids.iter().find_map(|(k, c)| match c {
            Child::One(n) if *k == key => Some(n),
            _ => None,
        })
    }

    fn many(&self, key: &str) -> &[Node] {
        self.kids
            .iter()
            .find_map(|(k, c)| match c {
                Child::Many(v) if *k == key => Some(v.as_slice()),
                _ => None,
            })
            .unwrap_or(&[])
    }

    fn children(&self) -> Vec<&Node> {
        let mut out = Vec::new();
        for (_, c) in &self.kids {
            match c {
                Child::One(n) => out.push(n),
                Child::Many(v) => out.extend(v.iter()),
            }
        }
        out
    }
}

type P<T = Node> = Option<T>;

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    last_end: usize,
    next_id: usize,
}

const DIRECTIVE_LOCATIONS: [&str; 19] = [
    "QUERY",
    "MUTATION",
    "SUBSCRIPTION",
    "FIELD",
    "FRAGMENT_DEFINITION",
    "FRAGMENT_SPREAD",
    "INLINE_FRAGMENT",
    "VARIABLE_DEFINITION",
    "SCHEMA",
    "SCALAR",
    "OBJECT",
    "FIELD_DEFINITION",
    "ARGUMENT_DEFINITION",
    "INTERFACE",
    "UNION",
    "ENUM",
    "ENUM_VALUE",
    "INPUT_OBJECT",
    "INPUT_FIELD_DEFINITION",
];

struct B {
    kind: &'static str,
    start: usize,
    value: String,
    flag: bool,
    operation: &'static str,
    kids: Vec<(&'static str, Child)>,
}

impl B {
    fn one(mut self, key: &'static str, n: Node) -> Self {
        self.kids.push((key, Child::One(n)));
        self
    }
    fn opt(mut self, key: &'static str, n: Option<Node>) -> Self {
        if let Some(n) = n {
            self.kids.push((key, Child::One(n)));
        }
        self
    }
    fn many(mut self, key: &'static str, v: Vec<Node>) -> Self {
        self.kids.push((key, Child::Many(v)));
        self
    }
    fn value(mut self, v: String) -> Self {
        self.value = v;
        self
    }
    fn flag(mut self, f: bool) -> Self {
        self.flag = f;
        self
    }
    fn op(mut self, o: &'static str) -> Self {
        self.operation = o;
        self
    }
}

impl Parser {
    fn tok(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn peek(&self, k: Tk) -> bool {
        self.tok().kind == k
    }

    fn advance(&mut self) {
        self.last_end = self.tokens[self.pos].end;
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
    }

    fn b(&self, kind: &'static str, start: usize) -> B {
        B {
            kind,
            start,
            value: String::new(),
            flag: false,
            operation: "",
            kids: Vec::new(),
        }
    }

    fn done(&mut self, b: B) -> Node {
        let id = self.next_id;
        self.next_id += 1;
        Node {
            kind: b.kind,
            start: b.start,
            end: self.last_end,
            id,
            value: b.value,
            flag: b.flag,
            operation: b.operation,
            kids: b.kids,
        }
    }

    fn expect(&mut self, k: Tk) -> P<usize> {
        if self.peek(k) {
            let start = self.tok().start;
            self.advance();
            Some(start)
        } else {
            None
        }
    }

    fn optional(&mut self, k: Tk) -> bool {
        self.expect(k).is_some()
    }

    fn is_keyword(&self, v: &str) -> bool {
        self.peek(Tk::Name) && self.tok().value == v
    }

    fn expect_keyword(&mut self, v: &str) -> P<()> {
        if self.is_keyword(v) {
            self.advance();
            Some(())
        } else {
            None
        }
    }

    fn optional_keyword(&mut self, v: &str) -> bool {
        self.expect_keyword(v).is_some()
    }

    fn many_of(&mut self, open: Tk, f: fn(&mut Self) -> P, close: Tk) -> P<Vec<Node>> {
        self.expect(open)?;
        let mut out = Vec::new();
        loop {
            out.push(f(self)?);
            if self.optional(close) {
                return Some(out);
            }
        }
    }

    fn optional_many(&mut self, open: Tk, f: fn(&mut Self) -> P, close: Tk) -> P<Vec<Node>> {
        if self.peek(open) {
            self.many_of(open, f, close)
        } else {
            Some(Vec::new())
        }
    }

    fn any_of(&mut self, open: Tk, close: Tk, f: &dyn Fn(&mut Self) -> P) -> P<Vec<Node>> {
        self.expect(open)?;
        let mut out = Vec::new();
        while !self.optional(close) {
            out.push(f(self)?);
        }
        Some(out)
    }

    fn delimited_many(&mut self, delim: Tk, f: fn(&mut Self) -> P) -> P<Vec<Node>> {
        self.optional(delim);
        let mut out = Vec::new();
        loop {
            out.push(f(self)?);
            if !self.optional(delim) {
                return Some(out);
            }
        }
    }

    fn name(&mut self) -> P {
        if !self.peek(Tk::Name) {
            return None;
        }
        let start = self.tok().start;
        let value = self.tok().value.clone();
        self.advance();
        let b = self.b("Name", start).value(value);
        Some(self.done(b))
    }

    fn document(&mut self) -> P {
        let start = self.tok().start;
        let defs = self.many_of(Tk::Sof, Self::definition, Tk::Eof)?;
        let b = self.b("Document", start).many("definitions", defs);
        Some(self.done(b))
    }

    fn lookahead(&self) -> &Token {
        &self.tokens[(self.pos + 1).min(self.tokens.len() - 1)]
    }

    fn definition(&mut self) -> P {
        if self.peek(Tk::BraceL) {
            return self.operation_definition();
        }
        let has_desc = self.peek_description();
        let kw = if has_desc {
            self.lookahead()
        } else {
            self.tok()
        };
        if kw.kind != Tk::Name {
            return None;
        }
        let kw = kw.value.clone();
        match kw.as_str() {
            "schema" => return self.schema_definition(),
            "scalar" => return self.scalar_definition(),
            "type" => return self.object_like("ObjectTypeDefinition", "type", true),
            "interface" => return self.object_like("InterfaceTypeDefinition", "interface", true),
            "union" => return self.union_definition(),
            "enum" => return self.enum_definition(),
            "input" => return self.object_like("InputObjectTypeDefinition", "input", true),
            "directive" => return self.directive_definition(),
            _ => {}
        }
        if has_desc {
            return None;
        }
        match kw.as_str() {
            "query" | "mutation" | "subscription" => self.operation_definition(),
            "fragment" => self.fragment_definition(),
            "extend" => self.extension(),
            _ => None,
        }
    }

    fn operation_definition(&mut self) -> P {
        let start = self.tok().start;
        if self.peek(Tk::BraceL) {
            let sel = self.selection_set()?;
            let b = self
                .b("OperationDefinition", start)
                .op("query")
                .many("variableDefinitions", Vec::new())
                .many("directives", Vec::new())
                .one("selectionSet", sel);
            return Some(self.done(b));
        }
        let op = self.operation_type()?;
        let name = if self.peek(Tk::Name) {
            Some(self.name()?)
        } else {
            None
        };
        let vars = self.optional_many(Tk::ParenL, Self::variable_definition, Tk::ParenR)?;
        let dirs = self.directives(false)?;
        let sel = self.selection_set()?;
        let b = self
            .b("OperationDefinition", start)
            .op(op)
            .opt("name", name)
            .many("variableDefinitions", vars)
            .many("directives", dirs)
            .one("selectionSet", sel);
        Some(self.done(b))
    }

    fn operation_type(&mut self) -> P<&'static str> {
        if !self.peek(Tk::Name) {
            return None;
        }
        let op = match self.tok().value.as_str() {
            "query" => "query",
            "mutation" => "mutation",
            "subscription" => "subscription",
            _ => return None,
        };
        self.advance();
        Some(op)
    }

    fn variable_definition(&mut self) -> P {
        let start = self.tok().start;
        let var = self.variable()?;
        self.expect(Tk::Colon)?;
        let ty = self.type_reference()?;
        let default = if self.optional(Tk::Equals) {
            Some(self.value_literal(true)?)
        } else {
            None
        };
        let dirs = self.directives(true)?;
        let b = self
            .b("VariableDefinition", start)
            .one("variable", var)
            .one("type", ty)
            .opt("defaultValue", default)
            .many("directives", dirs);
        Some(self.done(b))
    }

    fn variable(&mut self) -> P {
        let start = self.expect(Tk::Dollar)?;
        let name = self.name()?;
        let b = self.b("Variable", start).one("name", name);
        Some(self.done(b))
    }

    fn selection_set(&mut self) -> P {
        let start = self.tok().start;
        let sels = self.many_of(Tk::BraceL, Self::selection, Tk::BraceR)?;
        let b = self.b("SelectionSet", start).many("selections", sels);
        Some(self.done(b))
    }

    fn selection(&mut self) -> P {
        if self.peek(Tk::Spread) {
            self.fragment()
        } else {
            self.field()
        }
    }

    fn field(&mut self) -> P {
        let start = self.tok().start;
        let first = self.name()?;
        let (alias, name) = if self.optional(Tk::Colon) {
            (Some(first), self.name()?)
        } else {
            (None, first)
        };
        let args = self.arguments(false)?;
        let dirs = self.directives(false)?;
        let sel = if self.peek(Tk::BraceL) {
            Some(self.selection_set()?)
        } else {
            None
        };
        let b = self
            .b("Field", start)
            .opt("alias", alias)
            .one("name", name)
            .many("arguments", args)
            .many("directives", dirs)
            .opt("selectionSet", sel);
        Some(self.done(b))
    }

    fn arguments(&mut self, is_const: bool) -> P<Vec<Node>> {
        let f: fn(&mut Self) -> P = if is_const {
            |p| p.argument(true)
        } else {
            |p| p.argument(false)
        };
        self.optional_many(Tk::ParenL, f, Tk::ParenR)
    }

    fn argument(&mut self, is_const: bool) -> P {
        let start = self.tok().start;
        let name = self.name()?;
        self.expect(Tk::Colon)?;
        let value = self.value_literal(is_const)?;
        let b = self
            .b("Argument", start)
            .one("name", name)
            .one("value", value);
        Some(self.done(b))
    }

    fn fragment(&mut self) -> P {
        let start = self.expect(Tk::Spread)?;
        let has_type = self.optional_keyword("on");
        if !has_type && self.peek(Tk::Name) {
            let name = self.fragment_name()?;
            let dirs = self.directives(false)?;
            let b = self
                .b("FragmentSpread", start)
                .one("name", name)
                .many("directives", dirs);
            return Some(self.done(b));
        }
        let ty = if has_type {
            Some(self.named_type()?)
        } else {
            None
        };
        let dirs = self.directives(false)?;
        let sel = self.selection_set()?;
        let b = self
            .b("InlineFragment", start)
            .opt("typeCondition", ty)
            .many("directives", dirs)
            .one("selectionSet", sel);
        Some(self.done(b))
    }

    fn fragment_definition(&mut self) -> P {
        let start = self.tok().start;
        self.expect_keyword("fragment")?;
        let name = self.fragment_name()?;
        let vars = self.optional_many(Tk::ParenL, Self::variable_definition, Tk::ParenR)?;
        self.expect_keyword("on")?;
        let ty = self.named_type()?;
        let dirs = self.directives(false)?;
        let sel = self.selection_set()?;
        let b = self
            .b("FragmentDefinition", start)
            .one("name", name)
            .many("variableDefinitions", vars)
            .one("typeCondition", ty)
            .many("directives", dirs)
            .one("selectionSet", sel);
        Some(self.done(b))
    }

    fn fragment_name(&mut self) -> P {
        if self.tok().value == "on" && self.peek(Tk::Name) {
            return None;
        }
        self.name()
    }

    fn value_literal(&mut self, is_const: bool) -> P {
        let start = self.tok().start;
        match self.tok().kind {
            Tk::BracketL => {
                let values =
                    self.any_of(Tk::BracketL, Tk::BracketR, &|p| p.value_literal(is_const))?;
                let b = self.b("ListValue", start).many("values", values);
                Some(self.done(b))
            }
            Tk::BraceL => {
                let fields = self.any_of(Tk::BraceL, Tk::BraceR, &|p| p.object_field(is_const))?;
                let b = self.b("ObjectValue", start).many("fields", fields);
                Some(self.done(b))
            }
            Tk::Int | Tk::Float => {
                let kind = if self.peek(Tk::Int) {
                    "IntValue"
                } else {
                    "FloatValue"
                };
                let value = self.tok().value.clone();
                self.advance();
                let b = self.b(kind, start).value(value);
                Some(self.done(b))
            }
            Tk::Str | Tk::BlockStr => self.string_literal(),
            Tk::Name => {
                let value = self.tok().value.clone();
                self.advance();
                let b = match value.as_str() {
                    "true" => self.b("BooleanValue", start).flag(true),
                    "false" => self.b("BooleanValue", start).flag(false),
                    "null" => self.b("NullValue", start),
                    _ => self.b("EnumValue", start).value(value),
                };
                Some(self.done(b))
            }
            Tk::Dollar if !is_const => self.variable(),
            _ => None,
        }
    }

    fn string_literal(&mut self) -> P {
        let t = self.tok();
        let (start, block, value) = (t.start, t.kind == Tk::BlockStr, t.value.clone());
        self.advance();
        let b = self.b("StringValue", start).value(value).flag(block);
        Some(self.done(b))
    }

    fn object_field(&mut self, is_const: bool) -> P {
        let start = self.tok().start;
        let name = self.name()?;
        self.expect(Tk::Colon)?;
        let value = self.value_literal(is_const)?;
        let b = self
            .b("ObjectField", start)
            .one("name", name)
            .one("value", value);
        Some(self.done(b))
    }

    fn directives(&mut self, is_const: bool) -> P<Vec<Node>> {
        let mut out = Vec::new();
        while self.peek(Tk::At) {
            let start = self.expect(Tk::At)?;
            let name = self.name()?;
            let args = self.arguments(is_const)?;
            let b = self
                .b("Directive", start)
                .one("name", name)
                .many("arguments", args);
            out.push(self.done(b));
        }
        Some(out)
    }

    fn type_reference(&mut self) -> P {
        let start = self.tok().start;
        let ty = if self.optional(Tk::BracketL) {
            let inner = self.type_reference()?;
            self.expect(Tk::BracketR)?;
            let b = self.b("ListType", start).one("type", inner);
            self.done(b)
        } else {
            self.named_type()?
        };
        if self.optional(Tk::Bang) {
            let b = self.b("NonNullType", start).one("type", ty);
            return Some(self.done(b));
        }
        Some(ty)
    }

    fn named_type(&mut self) -> P {
        let start = self.tok().start;
        let name = self.name()?;
        let b = self.b("NamedType", start).one("name", name);
        Some(self.done(b))
    }

    fn peek_description(&self) -> bool {
        self.peek(Tk::Str) || self.peek(Tk::BlockStr)
    }

    fn description(&mut self) -> P<Option<Node>> {
        if self.peek_description() {
            Some(Some(self.string_literal()?))
        } else {
            Some(None)
        }
    }

    fn schema_definition(&mut self) -> P {
        let start = self.tok().start;
        let desc = self.description()?;
        self.expect_keyword("schema")?;
        let dirs = self.directives(true)?;
        let ops = self.many_of(Tk::BraceL, Self::operation_type_definition, Tk::BraceR)?;
        let b = self
            .b("SchemaDefinition", start)
            .opt("description", desc)
            .many("directives", dirs)
            .many("operationTypes", ops);
        Some(self.done(b))
    }

    fn operation_type_definition(&mut self) -> P {
        let start = self.tok().start;
        let op = self.operation_type()?;
        self.expect(Tk::Colon)?;
        let ty = self.named_type()?;
        let b = self
            .b("OperationTypeDefinition", start)
            .op(op)
            .one("type", ty);
        Some(self.done(b))
    }

    fn scalar_definition(&mut self) -> P {
        let start = self.tok().start;
        let desc = self.description()?;
        self.expect_keyword("scalar")?;
        let name = self.name()?;
        let dirs = self.directives(true)?;
        let b = self
            .b("ScalarTypeDefinition", start)
            .opt("description", desc)
            .one("name", name)
            .many("directives", dirs);
        Some(self.done(b))
    }

    fn implements(&mut self) -> P<Vec<Node>> {
        if self.optional_keyword("implements") {
            self.delimited_many(Tk::Amp, Self::named_type)
        } else {
            Some(Vec::new())
        }
    }

    fn object_like(&mut self, kind: &'static str, keyword: &str, definition: bool) -> P {
        let start = self.tok().start;
        let desc = if definition {
            self.description()?
        } else {
            self.expect_keyword("extend")?;
            None
        };
        self.expect_keyword(keyword)?;
        let name = self.name()?;
        let input = keyword == "input";
        let interfaces = if input {
            Vec::new()
        } else {
            self.implements()?
        };
        let dirs = self.directives(true)?;
        let fields = if input {
            self.optional_many(Tk::BraceL, Self::input_value_def, Tk::BraceR)?
        } else {
            self.optional_many(Tk::BraceL, Self::field_definition, Tk::BraceR)?
        };
        if !definition && interfaces.is_empty() && dirs.is_empty() && fields.is_empty() {
            return None;
        }
        let mut b = self
            .b(kind, start)
            .opt("description", desc)
            .one("name", name);
        if !input {
            b = b.many("interfaces", interfaces);
        }
        let b = b.many("directives", dirs).many("fields", fields);
        Some(self.done(b))
    }

    fn field_definition(&mut self) -> P {
        let start = self.tok().start;
        let desc = self.description()?;
        let name = self.name()?;
        let args = self.optional_many(Tk::ParenL, Self::input_value_def, Tk::ParenR)?;
        self.expect(Tk::Colon)?;
        let ty = self.type_reference()?;
        let dirs = self.directives(true)?;
        let b = self
            .b("FieldDefinition", start)
            .opt("description", desc)
            .one("name", name)
            .many("arguments", args)
            .one("type", ty)
            .many("directives", dirs);
        Some(self.done(b))
    }

    fn input_value_def(&mut self) -> P {
        let start = self.tok().start;
        let desc = self.description()?;
        let name = self.name()?;
        self.expect(Tk::Colon)?;
        let ty = self.type_reference()?;
        let default = if self.optional(Tk::Equals) {
            Some(self.value_literal(true)?)
        } else {
            None
        };
        let dirs = self.directives(true)?;
        let b = self
            .b("InputValueDefinition", start)
            .opt("description", desc)
            .one("name", name)
            .one("type", ty)
            .opt("defaultValue", default)
            .many("directives", dirs);
        Some(self.done(b))
    }

    fn union_members(&mut self) -> P<Vec<Node>> {
        if self.optional(Tk::Equals) {
            self.delimited_many(Tk::Pipe, Self::named_type)
        } else {
            Some(Vec::new())
        }
    }

    fn union_definition(&mut self) -> P {
        let start = self.tok().start;
        let desc = self.description()?;
        self.expect_keyword("union")?;
        let name = self.name()?;
        let dirs = self.directives(true)?;
        let types = self.union_members()?;
        let b = self
            .b("UnionTypeDefinition", start)
            .opt("description", desc)
            .one("name", name)
            .many("directives", dirs)
            .many("types", types);
        Some(self.done(b))
    }

    fn enum_values(&mut self) -> P<Vec<Node>> {
        self.optional_many(Tk::BraceL, Self::enum_value_definition, Tk::BraceR)
    }

    fn enum_definition(&mut self) -> P {
        let start = self.tok().start;
        let desc = self.description()?;
        self.expect_keyword("enum")?;
        let name = self.name()?;
        let dirs = self.directives(true)?;
        let values = self.enum_values()?;
        let b = self
            .b("EnumTypeDefinition", start)
            .opt("description", desc)
            .one("name", name)
            .many("directives", dirs)
            .many("values", values);
        Some(self.done(b))
    }

    fn enum_value_definition(&mut self) -> P {
        let start = self.tok().start;
        let desc = self.description()?;
        if matches!(self.tok().value.as_str(), "true" | "false" | "null") {
            return None;
        }
        let name = self.name()?;
        let dirs = self.directives(true)?;
        let b = self
            .b("EnumValueDefinition", start)
            .opt("description", desc)
            .one("name", name)
            .many("directives", dirs);
        Some(self.done(b))
    }

    fn directive_definition(&mut self) -> P {
        let start = self.tok().start;
        let desc = self.description()?;
        self.expect_keyword("directive")?;
        self.expect(Tk::At)?;
        let name = self.name()?;
        let args = self.optional_many(Tk::ParenL, Self::input_value_def, Tk::ParenR)?;
        let repeatable = self.optional_keyword("repeatable");
        self.expect_keyword("on")?;
        let locations = self.delimited_many(Tk::Pipe, |p| {
            let n = p.name()?;
            DIRECTIVE_LOCATIONS.contains(&n.value.as_str()).then_some(n)
        })?;
        let b = self
            .b("DirectiveDefinition", start)
            .opt("description", desc)
            .one("name", name)
            .many("arguments", args)
            .flag(repeatable)
            .many("locations", locations);
        Some(self.done(b))
    }

    fn extension(&mut self) -> P {
        let kw = self.lookahead();
        if kw.kind != Tk::Name {
            return None;
        }
        match kw.value.clone().as_str() {
            "schema" => {
                let start = self.tok().start;
                self.expect_keyword("extend")?;
                self.expect_keyword("schema")?;
                let dirs = self.directives(true)?;
                let ops =
                    self.optional_many(Tk::BraceL, Self::operation_type_definition, Tk::BraceR)?;
                if dirs.is_empty() && ops.is_empty() {
                    return None;
                }
                let b = self
                    .b("SchemaExtension", start)
                    .many("directives", dirs)
                    .many("operationTypes", ops);
                Some(self.done(b))
            }
            "scalar" => {
                let start = self.tok().start;
                self.expect_keyword("extend")?;
                self.expect_keyword("scalar")?;
                let name = self.name()?;
                let dirs = self.directives(true)?;
                if dirs.is_empty() {
                    return None;
                }
                let b = self
                    .b("ScalarTypeExtension", start)
                    .one("name", name)
                    .many("directives", dirs);
                Some(self.done(b))
            }
            "type" => self.object_like("ObjectTypeExtension", "type", false),
            "interface" => self.object_like("InterfaceTypeExtension", "interface", false),
            "input" => self.object_like("InputObjectTypeExtension", "input", false),
            "union" => {
                let start = self.tok().start;
                self.expect_keyword("extend")?;
                self.expect_keyword("union")?;
                let name = self.name()?;
                let dirs = self.directives(true)?;
                let types = self.union_members()?;
                if dirs.is_empty() && types.is_empty() {
                    return None;
                }
                let b = self
                    .b("UnionTypeExtension", start)
                    .one("name", name)
                    .many("directives", dirs)
                    .many("types", types);
                Some(self.done(b))
            }
            "enum" => {
                let start = self.tok().start;
                self.expect_keyword("extend")?;
                self.expect_keyword("enum")?;
                let name = self.name()?;
                let dirs = self.directives(true)?;
                let values = self.enum_values()?;
                if dirs.is_empty() && values.is_empty() {
                    return None;
                }
                let b = self
                    .b("EnumTypeExtension", start)
                    .one("name", name)
                    .many("directives", dirs)
                    .many("values", values);
                Some(self.done(b))
            }
            _ => None,
        }
    }
}

fn parse(text: &str) -> Option<(Node, Vec<Comment>, usize)> {
    let (tokens, comments) = lex(text)?;
    let mut p = Parser {
        tokens,
        pos: 0,
        last_end: 0,
        next_id: 0,
    };
    let doc = p.document()?;
    Some((doc, comments, p.next_id))
}

#[derive(Clone, Copy, PartialEq)]
enum Place {
    Leading,
    Trailing,
    Dangling,
}

struct Ctx<'a> {
    enclosing: Option<&'a Node>,
    preceding: Option<&'a Node>,
    following: Option<&'a Node>,
}

fn sorted_children(n: &Node) -> Vec<&Node> {
    let mut c = n.children();
    c.sort_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)));
    c
}

fn decorate<'a>(node: &'a Node, c: &Comment, enclosing: Option<&'a Node>) -> R<Ctx<'a>> {
    let children = sorted_children(node);
    let (mut preceding, mut following) = (None, None);
    let (mut left, mut right) = (0, children.len());
    while left < right {
        let mid = (left + right) / 2;
        let child = children[mid];
        if child.start <= c.start && c.end <= child.end {
            return decorate(child, c, Some(child));
        }
        if child.end <= c.start {
            preceding = Some(child);
            left = mid + 1;
            continue;
        }
        if c.end <= child.start {
            following = Some(child);
            right = mid;
            continue;
        }
        return bail("comment overlaps node");
    }
    Ok(Ctx {
        enclosing,
        preceding,
        following,
    })
}

fn is_all_empty_and_no_line_break(t: &str) -> bool {
    !t.chars()
        .any(|c| c == '\n' || c == '\u{2028}' || c == '\u{2029}' || !is_js_whitespace(c))
}

struct Attached {
    by_node: Vec<Vec<usize>>,
    place: Vec<Place>,
}

impl Attached {
    fn add(&mut self, node: &Node, ci: usize, place: Place) {
        self.by_node[node.id].push(ci);
        self.place[ci] = place;
    }
}

fn attach(root: &Node, comments: &[Comment], text: &str, count: usize) -> R<Attached> {
    let mut a = Attached {
        by_node: vec![Vec::new(); count],
        place: vec![Place::Dangling; comments.len()],
    };
    let mut ctxs = Vec::with_capacity(comments.len());
    for c in comments {
        ctxs.push(decorate(root, c, None)?);
    }
    let id = |n: Option<&Node>| n.map(|n| n.id);
    let mut ties: Vec<usize> = Vec::new();
    for (i, c) in comments.iter().enumerate() {
        let ctx = &ctxs[i];
        let mut start = c.start;
        if ctx.preceding.is_some() {
            for j in (0..i).rev() {
                if id(ctxs[j].preceding) != id(ctx.preceding)
                    || !is_all_empty_and_no_line_break(&text[comments[j].end..start])
                {
                    break;
                }
                start = comments[j].start;
            }
        }
        let own_line = has_newline(text, start, true);
        let mut end = c.end;
        if !own_line && ctx.following.is_some() {
            for j in i + 1..comments.len() {
                if id(ctxs[j].following) != id(ctx.following)
                    || !is_all_empty_and_no_line_break(&text[end..comments[j].start])
                {
                    break;
                }
                end = comments[j].end;
            }
        }
        let (first, second) = if own_line {
            (
                ctx.following.map(|n| (n, Place::Leading)),
                ctx.preceding.map(|n| (n, Place::Trailing)),
            )
        } else if has_newline(text, end, false) {
            (
                ctx.preceding.map(|n| (n, Place::Trailing)),
                ctx.following.map(|n| (n, Place::Leading)),
            )
        } else if let (Some(_), Some(f)) = (ctx.preceding, ctx.following) {
            if let Some(&last) = ties.last()
                && id(ctxs[last].following) != Some(f.id)
            {
                break_ties(&mut a, &mut ties, comments, &ctxs, text);
            }
            ties.push(i);
            continue;
        } else {
            (
                ctx.preceding.map(|n| (n, Place::Trailing)),
                ctx.following.map(|n| (n, Place::Leading)),
            )
        };
        match first.or(second) {
            Some((n, place)) => a.add(n, i, place),
            None => match ctx.enclosing {
                Some(n) => a.add(n, i, Place::Dangling),
                None => a.add(root, i, Place::Dangling),
            },
        }
    }
    break_ties(&mut a, &mut ties, comments, &ctxs, text);
    Ok(a)
}

fn break_ties(
    a: &mut Attached,
    ties: &mut Vec<usize>,
    comments: &[Comment],
    ctxs: &[Ctx<'_>],
    text: &str,
) {
    if ties.is_empty() {
        return;
    }
    let (Some(preceding), Some(following)) = (ctxs[ties[0]].preceding, ctxs[ties[0]].following)
    else {
        ties.clear();
        return;
    };
    let mut gap_end = following.start;
    let mut first_leading = ties.len();
    while first_leading > 0 {
        let c = &comments[ties[first_leading - 1]];
        let gap = &text[c.end..gap_end];
        if gap.chars().all(|ch| ch == '(' || is_js_whitespace(ch)) {
            gap_end = c.start;
            first_leading -= 1;
        } else {
            break;
        }
    }
    for (k, &ci) in ties.iter().enumerate() {
        if k < first_leading {
            a.add(preceding, ci, Place::Trailing);
        } else {
            a.add(following, ci, Place::Leading);
        }
    }
    for n in [preceding.id, following.id] {
        a.by_node[n].sort_by_key(|&ci| comments[ci].start);
    }
    ties.clear();
}

struct Printer<'a> {
    text: &'a str,
    opts: &'a Options,
    comments: &'a [Comment],
    attached: Attached,
    printed: Vec<bool>,
    ignored: Vec<bool>,
}

fn js_trim(s: &str) -> &str {
    s.trim_matches(is_js_whitespace)
}

fn args_group(printed: Vec<Doc>) -> Doc {
    group(concat(vec![
        s("("),
        indent(concat(vec![
            softline(),
            join(
                &concat(vec![if_break(empty(), s(", ")), softline()]),
                printed,
            ),
        ])),
        softline(),
        s(")"),
    ]))
}

fn block(open: &'static str, items: Vec<Doc>) -> Doc {
    concat(vec![
        s(open),
        indent(concat(vec![hardline(), join(&hardline(), items)])),
        hardline(),
        s("}"),
    ])
}

impl Printer<'_> {
    fn print(&mut self, n: &Node) -> R {
        let ignore = self.attached.by_node[n.id]
            .iter()
            .any(|&ci| js_trim(&self.comments[ci].value) == "prettier-ignore");
        let doc = if ignore {
            for (i, c) in self.comments.iter().enumerate() {
                if c.start >= n.start && c.end <= n.end {
                    self.ignored[i] = true;
                }
            }
            s(self.text[n.start..n.end].to_string())
        } else {
            self.generic(n)?
        };
        Ok(self.print_comments(n, doc))
    }

    fn print_opt(&mut self, n: Option<&Node>) -> R {
        match n {
            Some(n) => self.print(n),
            None => Ok(empty()),
        }
    }

    fn print_comment(&mut self, ci: usize) -> Doc {
        self.printed[ci] = true;
        s(format!(
            "#{}",
            self.comments[ci].value.trim_end_matches(is_js_whitespace)
        ))
    }

    fn print_comments(&mut self, n: &Node, doc: Doc) -> Doc {
        let list: Vec<usize> = self.attached.by_node[n.id]
            .iter()
            .copied()
            .filter(|&ci| !self.ignored[ci])
            .collect();
        if list.is_empty() {
            return doc;
        }
        let text = self.text;
        let mut leading = Vec::new();
        let mut trailing = Vec::new();
        let mut prev_suffix = false;
        for ci in list {
            let c = &self.comments[ci];
            let (start, end) = (c.start, c.end);
            match self.attached.place[ci] {
                Place::Leading => {
                    let mut parts = vec![self.print_comment(ci), hardline()];
                    let idx = skip_newline(text, skip_spaces(text, Some(end), false), false);
                    if let Some(i) = idx
                        && has_newline(text, i, false)
                    {
                        parts.push(hardline());
                    }
                    leading.push(concat(parts));
                }
                Place::Trailing => {
                    let printed = self.print_comment(ci);
                    if prev_suffix || has_newline(text, start, true) {
                        let empty_before = crate::comments::is_previous_line_empty(text, start);
                        trailing.push(line_suffix(concat(vec![
                            hardline(),
                            if empty_before { hardline() } else { empty() },
                            printed,
                        ])));
                    } else {
                        trailing.push(concat(vec![
                            line_suffix(concat(vec![s(" "), printed])),
                            break_parent(),
                        ]));
                    }
                    prev_suffix = true;
                }
                Place::Dangling => {}
            }
        }
        inherit_label(doc, |d| concat(vec![concat(leading), d, concat(trailing)]))
    }

    fn sequence(&mut self, items: &[Node]) -> R<Vec<Doc>> {
        let mut out = Vec::with_capacity(items.len());
        for (i, n) in items.iter().enumerate() {
            let printed = self.print(n)?;
            if i + 1 < items.len() && is_next_line_empty(self.text, n.end) {
                out.push(concat(vec![printed, hardline()]));
            } else {
                out.push(printed);
            }
        }
        Ok(out)
    }

    fn map(&mut self, items: &[Node]) -> R<Vec<Doc>> {
        items.iter().map(|n| self.print(n)).collect()
    }

    fn directives(&mut self, n: &Node) -> R {
        let dirs = n.many("directives");
        if dirs.is_empty() {
            return Ok(empty());
        }
        let printed = join(&line(), self.map(dirs)?);
        if matches!(n.kind, "FragmentDefinition" | "OperationDefinition") {
            return Ok(group(concat(vec![line(), printed])));
        }
        Ok(concat(vec![
            s(" "),
            group(indent(concat(vec![softline(), printed]))),
        ]))
    }

    fn description(&mut self, n: &Node) -> R {
        let Some(d) = n.one("description") else {
            return Ok(empty());
        };
        let printed = self.print(d)?;
        let sep = if n.kind == "InputValueDefinition" && !d.flag {
            line()
        } else {
            hardline()
        };
        Ok(concat(vec![printed, sep]))
    }

    fn arguments(&mut self, n: &Node) -> R {
        let args = n.many("arguments");
        if args.is_empty() {
            return Ok(empty());
        }
        let printed = self.sequence(args)?;
        Ok(args_group(printed))
    }

    fn variable_definitions(&mut self, n: &Node) -> R {
        let vars = n.many("variableDefinitions");
        if vars.is_empty() {
            return Ok(empty());
        }
        let printed = self.map(vars)?;
        Ok(args_group(printed))
    }

    fn interfaces(&mut self, n: &Node) -> R {
        let list = n.many("interfaces");
        let printed = self.map(list)?;
        let mut parts = Vec::new();
        for (i, p) in printed.into_iter().enumerate() {
            parts.push(p);
            if let Some(next) = list.get(i + 1) {
                let between = &self.text[list[i].end..next.start];
                parts.push(s(" &"));
                parts.push(if between.contains('#') {
                    line()
                } else {
                    s(" ")
                });
            }
        }
        Ok(concat(parts))
    }

    fn generic(&mut self, n: &Node) -> R {
        Ok(match n.kind {
            "Document" => {
                let defs = self.sequence(n.many("definitions"))?;
                concat(vec![join(&hardline(), defs), hardline()])
            }
            "OperationDefinition" => {
                let has_operation = self.text.as_bytes().get(n.start) != Some(&b'{');
                let name = n.one("name");
                let mut parts = Vec::new();
                parts.push(if has_operation {
                    s(n.operation)
                } else {
                    empty()
                });
                if has_operation && name.is_some() {
                    parts.push(s(" "));
                    parts.push(self.print_opt(name)?);
                }
                if has_operation && name.is_none() && !n.many("variableDefinitions").is_empty() {
                    parts.push(s(" "));
                }
                parts.push(self.variable_definitions(n)?);
                parts.push(self.directives(n)?);
                if has_operation || name.is_some() {
                    parts.push(s(" "));
                }
                parts.push(self.print_opt(n.one("selectionSet"))?);
                concat(parts)
            }
            "FragmentDefinition" => concat(vec![
                s("fragment "),
                self.print_opt(n.one("name"))?,
                self.variable_definitions(n)?,
                s(" on "),
                self.print_opt(n.one("typeCondition"))?,
                self.directives(n)?,
                s(" "),
                self.print_opt(n.one("selectionSet"))?,
            ]),
            "SelectionSet" => {
                let sels = self.sequence(n.many("selections"))?;
                block("{", sels)
            }
            "Field" => {
                let mut parts = Vec::new();
                if let Some(alias) = n.one("alias") {
                    parts.push(self.print(alias)?);
                    parts.push(s(": "));
                }
                parts.push(self.print_opt(n.one("name"))?);
                parts.push(self.arguments(n)?);
                parts.push(self.directives(n)?);
                let sel = n.one("selectionSet");
                if sel.is_some() {
                    parts.push(s(" "));
                }
                parts.push(self.print_opt(sel)?);
                group(concat(parts))
            }
            "Name" | "IntValue" | "FloatValue" | "EnumValue" => s(n.value.clone()),
            "StringValue" => {
                if n.flag {
                    let replaced = n.value.replace("\"\"\"", "\\\"\"\"");
                    let mut lines: Vec<&str> = replaced.split('\n').collect();
                    if lines.len() == 1 {
                        lines[0] = js_trim(lines[0]);
                    }
                    if lines.iter().all(|l| l.is_empty()) {
                        lines.clear();
                    }
                    let mut docs = vec![s("\"\"\"")];
                    docs.extend(lines.into_iter().map(|l| s(l.to_string())));
                    docs.push(s("\"\"\""));
                    join(&hardline(), docs)
                } else {
                    let mut out = String::with_capacity(n.value.len() + 2);
                    out.push('"');
                    for c in n.value.chars() {
                        match c {
                            '"' | '\\' => {
                                out.push('\\');
                                out.push(c);
                            }
                            '\n' => out.push_str("\\n"),
                            c => out.push(c),
                        }
                    }
                    out.push('"');
                    s(out)
                }
            }
            "BooleanValue" => s(if n.flag { "true" } else { "false" }),
            "NullValue" => s("null"),
            "Variable" => concat(vec![s("$"), self.print_opt(n.one("name"))?]),
            "ListValue" => {
                let values = self.map(n.many("values"))?;
                group(concat(vec![
                    s("["),
                    indent(concat(vec![
                        softline(),
                        join(
                            &concat(vec![if_break(empty(), s(", ")), softline()]),
                            values,
                        ),
                    ])),
                    softline(),
                    s("]"),
                ]))
            }
            "ObjectValue" => {
                let fields = n.many("fields");
                let space = if self.opts.bracket_spacing && !fields.is_empty() {
                    " "
                } else {
                    ""
                };
                let printed = self.map(fields)?;
                group(concat(vec![
                    s("{"),
                    s(space),
                    indent(concat(vec![
                        softline(),
                        join(
                            &concat(vec![if_break(empty(), s(", ")), softline()]),
                            printed,
                        ),
                    ])),
                    softline(),
                    if_break(empty(), s(space)),
                    s("}"),
                ]))
            }
            "ObjectField" | "Argument" => concat(vec![
                self.print_opt(n.one("name"))?,
                s(": "),
                self.print_opt(n.one("value"))?,
            ]),
            "Directive" => concat(vec![
                s("@"),
                self.print_opt(n.one("name"))?,
                self.arguments(n)?,
            ]),
            "NamedType" => self.print_opt(n.one("name"))?,
            "VariableDefinition" | "InputValueDefinition" => {
                let mut parts = Vec::new();
                if n.kind == "InputValueDefinition" {
                    parts.push(self.description(n)?);
                    parts.push(self.print_opt(n.one("name"))?);
                } else {
                    parts.push(self.print_opt(n.one("variable"))?);
                }
                parts.push(s(": "));
                parts.push(self.print_opt(n.one("type"))?);
                if let Some(d) = n.one("defaultValue") {
                    parts.push(s(" = "));
                    parts.push(self.print(d)?);
                }
                parts.push(self.directives(n)?);
                concat(parts)
            }
            "ObjectTypeExtension"
            | "ObjectTypeDefinition"
            | "InputObjectTypeExtension"
            | "InputObjectTypeDefinition"
            | "InterfaceTypeExtension"
            | "InterfaceTypeDefinition" => {
                let kind = n.kind;
                let mut parts = Vec::new();
                if kind.ends_with("TypeDefinition") {
                    parts.push(self.description(n)?);
                } else {
                    parts.push(s("extend "));
                }
                parts.push(s(if kind.starts_with("ObjectType") {
                    "type"
                } else if kind.starts_with("InputObjectType") {
                    "input"
                } else {
                    "interface"
                }));
                parts.push(s(" "));
                parts.push(self.print_opt(n.one("name"))?);
                if !kind.starts_with("InputObjectType") && !n.many("interfaces").is_empty() {
                    parts.push(s(" implements "));
                    parts.push(self.interfaces(n)?);
                }
                parts.push(self.directives(n)?);
                let fields = n.many("fields");
                if !fields.is_empty() {
                    let printed = self.sequence(fields)?;
                    parts.push(block(" {", printed));
                }
                concat(parts)
            }
            "FieldDefinition" => concat(vec![
                self.description(n)?,
                self.print_opt(n.one("name"))?,
                self.arguments(n)?,
                s(": "),
                self.print_opt(n.one("type"))?,
                self.directives(n)?,
            ]),
            "DirectiveDefinition" => {
                let mut parts = vec![
                    self.description(n)?,
                    s("directive "),
                    s("@"),
                    self.print_opt(n.one("name"))?,
                    self.arguments(n)?,
                    s(if n.flag { " repeatable" } else { "" }),
                    s(" on "),
                ];
                let locations = self.map(n.many("locations"))?;
                parts.push(join(&s(" | "), locations));
                concat(parts)
            }
            "EnumTypeExtension" | "EnumTypeDefinition" => {
                let mut parts = vec![
                    self.description(n)?,
                    s(if n.kind == "EnumTypeExtension" {
                        "extend "
                    } else {
                        ""
                    }),
                    s("enum "),
                    self.print_opt(n.one("name"))?,
                    self.directives(n)?,
                ];
                let values = n.many("values");
                if !values.is_empty() {
                    let printed = self.sequence(values)?;
                    parts.push(block(" {", printed));
                }
                concat(parts)
            }
            "EnumValueDefinition" => concat(vec![
                self.description(n)?,
                self.print_opt(n.one("name"))?,
                self.directives(n)?,
            ]),
            "SchemaExtension" => {
                let mut parts = vec![s("extend schema"), self.directives(n)?];
                let ops = n.many("operationTypes");
                if !ops.is_empty() {
                    let printed = self.sequence(ops)?;
                    parts.push(block(" {", printed));
                }
                concat(parts)
            }
            "SchemaDefinition" => {
                let mut parts = vec![
                    self.description(n)?,
                    s("schema"),
                    self.directives(n)?,
                    s(" {"),
                ];
                let ops = n.many("operationTypes");
                if !ops.is_empty() {
                    let printed = self.sequence(ops)?;
                    parts.push(indent(concat(vec![hardline(), join(&hardline(), printed)])));
                }
                parts.push(hardline());
                parts.push(s("}"));
                concat(parts)
            }
            "OperationTypeDefinition" => concat(vec![
                s(n.operation),
                s(": "),
                self.print_opt(n.one("type"))?,
            ]),
            "FragmentSpread" => concat(vec![
                s("..."),
                self.print_opt(n.one("name"))?,
                self.directives(n)?,
            ]),
            "InlineFragment" => {
                let mut parts = vec![s("...")];
                if let Some(t) = n.one("typeCondition") {
                    parts.push(s(" on "));
                    parts.push(self.print(t)?);
                }
                parts.push(self.directives(n)?);
                parts.push(s(" "));
                parts.push(self.print_opt(n.one("selectionSet"))?);
                concat(parts)
            }
            "UnionTypeExtension" | "UnionTypeDefinition" => {
                let desc = self.description(n)?;
                let mut inner = vec![
                    s(if n.kind == "UnionTypeExtension" {
                        "extend "
                    } else {
                        ""
                    }),
                    s("union "),
                    self.print_opt(n.one("name"))?,
                    self.directives(n)?,
                ];
                let types = n.many("types");
                if !types.is_empty() {
                    let printed = self.map(types)?;
                    let sep = concat(vec![line(), s("| ")]);
                    inner.push(s(" ="));
                    inner.push(if_break(empty(), s(" ")));
                    inner.push(indent(concat(vec![
                        if_break(concat(vec![line(), s("| ")]), empty()),
                        join(&sep, printed),
                    ])));
                }
                group(concat(vec![desc, group(concat(inner))]))
            }
            "ScalarTypeExtension" | "ScalarTypeDefinition" => concat(vec![
                self.description(n)?,
                s(if n.kind == "ScalarTypeExtension" {
                    "extend "
                } else {
                    ""
                }),
                s("scalar "),
                self.print_opt(n.one("name"))?,
                self.directives(n)?,
            ]),
            "NonNullType" => concat(vec![self.print_opt(n.one("type"))?, s("!")]),
            "ListType" => concat(vec![s("["), self.print_opt(n.one("type"))?, s("]")]),
            other => return bail(format!("graphql node {other}")),
        })
    }
}

/// Prettier's `graphql` parser and printer: the doc for `text`, or why this port cannot print it.
pub fn print(text: &str, opts: &Options) -> R {
    let Some((root, comments, count)) = parse(text) else {
        return bail("graphql parse error");
    };
    let attached = attach(&root, &comments, text, count)?;
    let mut p = Printer {
        text,
        opts,
        comments: &comments,
        attached,
        printed: vec![false; comments.len()],
        ignored: vec![false; comments.len()],
    };
    let doc = p.print(&root)?;
    if p.printed.iter().zip(&p.ignored).any(|(a, b)| !a && !b) {
        return bail("comment not printed");
    }
    Ok(doc)
}
