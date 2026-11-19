//! `src/language-yaml/`: printer-yaml.js, print/*.js and utils.js, on the tree `ast.rs` builds.

mod ast;
mod cst;

use ast::{Tree, Y};
use cst::Chomp;

use crate::comments::{is_js_whitespace, is_previous_line_empty};
use crate::doc::*;
use crate::options::{Options, ProseWrap, TrailingComma};
use crate::printer::{R, bail};

fn js_trim(s: &str) -> &str {
    s.trim_matches(is_js_whitespace)
}

fn align_spaces(width: usize, doc: Doc) -> Doc {
    Doc::Align(Align::Text(" ".repeat(width)), Box::new(doc))
}

fn fill_words(words: Vec<String>) -> Doc {
    let mut parts = Vec::new();
    for (i, w) in words.into_iter().enumerate() {
        if i > 0 {
            parts.push(line());
        }
        parts.push(s(w));
    }
    fill(parts)
}

fn split_with_single_space(text: &str) -> Vec<String> {
    let mut pieces: Vec<&str> = Vec::new();
    let mut last = 0;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b' ' {
            let st = i;
            while i < bytes.len() && bytes[i] == b' ' {
                i += 1;
            }
            pieces.push(&text[last..st]);
            pieces.push(&text[st..i]);
            last = i;
        } else {
            i += 1;
        }
    }
    pieces.push(&text[last..]);
    let mut parts: Vec<String> = Vec::new();
    let mut last_part: Option<&str> = None;
    for part in pieces {
        if part != " " {
            if last_part == Some(" ") {
                parts.push(part.to_string());
            } else {
                let prev = parts.pop().unwrap_or_default();
                parts.push(prev + part);
            }
        } else if last_part.is_none() {
            parts.insert(0, String::new());
        }
        last_part = Some(part);
    }
    if last_part == Some(" ") {
        let prev = parts.pop().unwrap_or_default();
        parts.push(prev + " ");
    }
    if parts.first().is_some_and(String::is_empty) {
        parts.remove(0);
        let first = if parts.is_empty() {
            String::new()
        } else {
            parts.remove(0)
        };
        parts.insert(0, format!(" {first}"));
    }
    parts
}

fn js_slice_from(s: &str, units: usize) -> &str {
    let mut count = 0;
    for (i, ch) in s.char_indices() {
        if count >= units {
            return &s[i..];
        }
        count += ch.len_utf16();
    }
    ""
}

struct P<'a> {
    t: &'a Tree,
    text: &'a str,
    opts: &'a Options,
    printed_lines: std::collections::HashSet<usize>,
    ids: usize,
}

impl P<'_> {
    fn n(&self, id: usize) -> &ast::U {
        &self.t.n[id]
    }

    fn slice(&self, a: usize, b: usize) -> &str {
        let len = self.text.len();
        self.text.get(a.min(len)..b.min(len)).unwrap_or("")
    }

    fn has_comments(&self, id: usize) -> bool {
        let n = self.n(id);
        !n.leading.is_empty()
            || !n.middle.is_empty()
            || n.indicator.is_some()
            || n.trailing.is_some()
            || !n.end_comments.is_empty()
    }

    fn is_empty_node(&self, id: usize) -> bool {
        self.n(id).kids.is_empty() && !self.has_comments(id)
    }

    fn content(&self, id: usize) -> Option<usize> {
        self.n(id).kids.first().copied()
    }

    fn is_inline(&self, id: Option<usize>) -> bool {
        id.is_none_or(|x| {
            matches!(
                self.n(x).ty,
                Y::Plain
                    | Y::QuoteDouble
                    | Y::QuoteSingle
                    | Y::Alias
                    | Y::FlowMapping
                    | Y::FlowSequence
            )
        })
    }

    fn last_descendant(&self, mut id: usize) -> usize {
        while let Some(&k) = self.n(id).kids.last() {
            id = k;
        }
        id
    }

    fn should_print_end_comments(&self, id: usize) -> bool {
        let n = self.n(id);
        !n.end_comments.is_empty()
            && !matches!(
                n.ty,
                Y::DocumentHead | Y::DocumentBody | Y::FlowMapping | Y::FlowSequence
            )
    }

    fn is_next_line_empty(&self, id: usize) -> bool {
        let end = self.n(id).end.offset;
        let bytes = self.text.as_bytes();
        let mut from = end.saturating_sub(1).min(bytes.len());
        while from > 0 && from < bytes.len() && (bytes[from] & 0xC0) == 0x80 {
            from -= 1;
        }
        let mut newlines = 0;
        for ch in self.text.get(from..).unwrap_or("").chars() {
            if ch == '\n' {
                newlines += 1;
            }
            if newlines == 1 && !is_js_whitespace(ch) {
                return false;
            }
            if newlines == 2 {
                return true;
            }
        }
        false
    }

    fn next_empty_line(&mut self, id: usize) -> Doc {
        let line = self.n(id).end.line;
        if self.printed_lines.insert(line)
            && self.is_next_line_empty(id)
            && !self
                .n(id)
                .parent
                .is_some_and(|p| self.should_print_end_comments(p))
        {
            return softline();
        }
        empty()
    }

    fn is_prettier_ignore(&self, c: usize) -> bool {
        js_trim(&self.n(c).value) == "prettier-ignore"
    }

    fn has_prettier_ignore(&self, id: usize) -> bool {
        let n = self.n(id);
        if n.ty == Y::DocumentBody {
            let Some(doc) = n.parent else {
                return false;
            };
            let head = self.n(doc).kids[0];
            return self
                .n(head)
                .end_comments
                .last()
                .is_some_and(|&c| self.is_prettier_ignore(c));
        }
        n.leading
            .last()
            .is_some_and(|&c| self.is_prettier_ignore(c))
    }

    fn print(&mut self, id: usize, last: bool) -> R {
        let n = self.n(id).clone();
        let mut parts = Vec::new();
        if n.ty != Y::MappingValue && !n.leading.is_empty() {
            let mut cs = Vec::new();
            for &c in &n.leading {
                cs.push(self.print(c, false)?);
            }
            parts.push(join(&hardline(), cs));
            parts.push(hardline());
        }
        if let Some(t) = n.tag {
            parts.push(self.print(t, false)?);
        }
        if n.tag.is_some() && n.anchor.is_some() {
            parts.push(s(" "));
        }
        if let Some(a) = n.anchor {
            parts.push(self.print(a, false)?);
        }
        let is_last = !matches!(n.ty, Y::Tag | Y::Anchor | Y::Comment) && last;
        let mut next_empty = empty();
        if matches!(
            n.ty,
            Y::Mapping | Y::Sequence | Y::Comment | Y::Directive | Y::MappingItem | Y::SequenceItem
        ) && !is_last
        {
            next_empty = self.next_empty_line(id);
        }
        if n.tag.is_some() || n.anchor.is_some() {
            if matches!(n.ty, Y::Sequence | Y::Mapping) && n.middle.is_empty() {
                parts.push(hardline());
            } else {
                parts.push(s(" "));
            }
        }
        if !n.middle.is_empty() {
            let mut cs = Vec::new();
            for &c in &n.middle {
                cs.push(self.print(c, false)?);
            }
            parts.push(concat(vec![
                if n.middle.len() == 1 {
                    empty()
                } else {
                    hardline()
                },
                join(&hardline(), cs),
                hardline(),
            ]));
        }
        if self.has_prettier_ignore(id) {
            let raw = self
                .slice(n.start.offset, n.end.offset)
                .trim_end_matches(is_js_whitespace)
                .to_string();
            parts.push(replace_end_of_line(&raw));
        } else {
            parts.push(group(self.print_node(id, last)?));
        }
        if let Some(tc) = n.trailing
            && !matches!(n.ty, Y::Document | Y::DocumentHead)
        {
            let parent_is_key = n.parent.is_some_and(|p| {
                self.n(p).ty == Y::MappingKey
                    && self
                        .n(p)
                        .parent
                        .and_then(|x| self.n(x).parent)
                        .is_some_and(|x| self.n(x).ty == Y::Mapping)
            });
            parts.push(line_suffix(concat(vec![
                if n.ty == Y::MappingValue && n.kids.is_empty() {
                    empty()
                } else {
                    s(" ")
                },
                if parent_is_key && self.is_inline(Some(id)) {
                    empty()
                } else {
                    break_parent()
                },
                self.print(tc, false)?,
            ])));
        }
        if self.should_print_end_comments(id) {
            let mut cs = Vec::new();
            for &c in &n.end_comments {
                let empty_before = is_previous_line_empty(self.text, self.n(c).start.offset);
                cs.push(concat(vec![
                    if empty_before { hardline() } else { empty() },
                    self.print(c, false)?,
                ]));
            }
            parts.push(align_spaces(
                if n.ty == Y::SequenceItem { 2 } else { 0 },
                concat(vec![hardline(), join(&hardline(), cs)]),
            ));
        }
        parts.push(next_empty);
        Ok(concat(parts))
    }

    fn print_kids(&mut self, id: usize, last: bool) -> R<Vec<Doc>> {
        let kids = self.n(id).kids.clone();
        let len = kids.len();
        let mut out = Vec::new();
        for (i, k) in kids.into_iter().enumerate() {
            out.push(self.print(k, last && i + 1 == len)?);
        }
        Ok(out)
    }

    fn should_print_doc_end_marker(&self, doc: usize, next: Option<usize>) -> bool {
        self.n(doc).trailing.is_some()
            || next.is_some_and(|nx| {
                let head = self.n(nx).kids[0];
                !self.n(head).kids.is_empty() || !self.n(head).end_comments.is_empty()
            })
    }

    fn print_node(&mut self, id: usize, last: bool) -> R {
        let n = self.n(id).clone();
        Ok(match n.ty {
            Y::Root => {
                let mut parts = Vec::new();
                let len = n.kids.len();
                for (i, &doc) in n.kids.iter().enumerate() {
                    let next = n.kids.get(i + 1).copied();
                    if i > 0 {
                        parts.push(hardline());
                    }
                    parts.push(self.print(doc, last && i + 1 == len)?);
                    if self.should_print_doc_end_marker(doc, next) {
                        parts.push(hardline());
                        parts.push(s("..."));
                        if let Some(tc) = self.n(doc).trailing {
                            parts.push(s(" "));
                            parts.push(self.print(tc, false)?);
                        }
                    } else if let Some(nx) = next {
                        let head = self.n(nx).kids[0];
                        if self.n(head).trailing.is_none() {
                            parts.push(hardline());
                            parts.push(s("---"));
                        }
                    }
                }
                let ld = self.last_descendant(id);
                if !(matches!(self.n(ld).ty, Y::BlockFolded | Y::BlockLiteral)
                    && self.n(ld).chomping == Chomp::Keep)
                {
                    parts.push(hardline());
                }
                concat(parts)
            }
            Y::Document => {
                let (head, body) = (n.kids[0], n.kids[1]);
                let root = n.parent.unwrap_or(id);
                let is_first = self.n(root).kids.first() == Some(&id);
                let hn = self.n(head).clone();
                let start = n.start.offset;
                let four: String = self
                    .text
                    .get(start.min(self.text.len())..)
                    .unwrap_or("")
                    .encode_utf16()
                    .take(4)
                    .map(|u| char::from_u32(u32::from(u)).unwrap_or('\u{FFFD}'))
                    .collect();
                let four: Vec<char> = four.chars().collect();
                let marker_text = (0..four.len().saturating_sub(2)).any(|i| {
                    four[i..i + 3] == ['-', '-', '-']
                        && (i + 3 == four.len() || is_js_whitespace(four[i + 3]))
                });
                let head_marker = (is_first && marker_text)
                    || !hn.kids.is_empty()
                    || !hn.end_comments.is_empty()
                    || hn.trailing.is_some();
                let mut parts = Vec::new();
                if head_marker {
                    if !hn.kids.is_empty() || !hn.end_comments.is_empty() {
                        parts.push(self.print(head, last)?);
                    }
                    if let Some(tc) = hn.trailing {
                        parts.push(concat(vec![s("---"), s(" "), self.print(tc, false)?]));
                    } else {
                        parts.push(s("---"));
                    }
                }
                let bn = self.n(body);
                if !bn.kids.is_empty() || !bn.end_comments.is_empty() {
                    parts.push(self.print(body, last)?);
                }
                join(&hardline(), parts)
            }
            Y::DocumentHead => {
                let mut parts = self.print_kids(id, last)?;
                for &c in &n.end_comments {
                    parts.push(self.print(c, false)?);
                }
                join(&hardline(), parts)
            }
            Y::DocumentBody => {
                let mut sep = empty();
                if !n.kids.is_empty() && !n.end_comments.is_empty() {
                    let ld = self.last_descendant(id);
                    if matches!(self.n(ld).ty, Y::BlockFolded | Y::BlockLiteral) {
                        if self.n(ld).chomping != Chomp::Keep {
                            sep = concat(vec![hardline(), hardline()]);
                        }
                    } else {
                        sep = hardline();
                    }
                }
                let kids = self.print_kids(id, last)?;
                let mut ends = Vec::new();
                for &c in &n.end_comments {
                    ends.push(self.print(c, false)?);
                }
                concat(vec![join(&hardline(), kids), sep, join(&hardline(), ends)])
            }
            Y::Directive => {
                let mut words = vec![s(n.name.clone())];
                words.extend(n.params.iter().map(|p| s(p.clone())));
                concat(vec![s("%"), join(&s(" "), words)])
            }
            Y::Comment => concat(vec![s("#"), s(n.value.clone())]),
            Y::Alias => concat(vec![s("*"), s(n.value.clone())]),
            Y::Tag => s(self.slice(n.start.offset, n.end.offset).to_string()),
            Y::Anchor => concat(vec![s("&"), s(n.value.clone())]),
            Y::Plain => {
                let raw = self.slice(n.start.offset, n.end.offset).to_string();
                self.flow_scalar(Y::Plain, &raw)
            }
            Y::QuoteDouble | Y::QuoteSingle => {
                let raw = self
                    .slice(n.start.offset + 1, n.end.offset.saturating_sub(1))
                    .to_string();
                let double = n.ty == Y::QuoteDouble;
                let backslash_other = raw
                    .as_bytes()
                    .windows(2)
                    .any(|w| w[0] == b'\\' && w[1] != b'"');
                if (!double && raw.contains('\\')) || (double && backslash_other) {
                    let q = if double { "\"" } else { "'" };
                    return Ok(concat(vec![s(q), self.flow_scalar(n.ty, &raw), s(q)]));
                }
                if raw.contains('"') {
                    let inner = if double {
                        raw.replace("\\\"", "\"").replace('\'', "''")
                    } else {
                        raw.clone()
                    };
                    return Ok(concat(vec![s("'"), self.flow_scalar(n.ty, &inner), s("'")]));
                }
                if raw.contains('\'') {
                    let inner = if double {
                        raw.clone()
                    } else {
                        raw.replace("''", "'")
                    };
                    return Ok(concat(vec![
                        s("\""),
                        self.flow_scalar(n.ty, &inner),
                        s("\""),
                    ]));
                }
                let q = if self.opts.single_quote { "'" } else { "\"" };
                concat(vec![s(q), self.flow_scalar(n.ty, &raw), s(q)])
            }
            Y::BlockFolded | Y::BlockLiteral => self.print_block(id, last)?,
            Y::Mapping | Y::Sequence => {
                let kids = self.print_kids(id, last)?;
                join(&hardline(), kids)
            }
            Y::SequenceItem => {
                let content = match self.content(id) {
                    Some(c) => self.print(c, last)?,
                    None => empty(),
                };
                concat(vec![s("- "), align_spaces(2, content)])
            }
            Y::MappingKey | Y::MappingValue => match self.content(id) {
                Some(c) => self.print(c, last)?,
                None => empty(),
            },
            Y::MappingItem | Y::FlowMappingItem => self.print_mapping_item(id, last)?,
            Y::FlowMapping | Y::FlowSequence => self.print_flow(id, last)?,
            Y::FlowSequenceItem => match self.content(id) {
                Some(c) => self.print(c, last)?,
                None => return bail("empty flow sequence item"),
            },
        })
    }

    fn flow_scalar(&self, ty: Y, content: &str) -> Doc {
        let lines = self.flow_scalar_lines(ty, content);
        join(&hardline(), lines.into_iter().map(fill_words).collect())
    }

    fn flow_scalar_lines(&self, ty: Y, content: &str) -> Vec<Vec<String>> {
        let split: Vec<&str> = content.split('\n').collect();
        let last = split.len() - 1;
        let raw: Vec<&str> = split
            .iter()
            .enumerate()
            .map(|(i, l)| {
                if i == 0 && i == last {
                    *l
                } else if i != 0 && i != last {
                    js_trim(l)
                } else if i == 0 {
                    l.trim_end_matches(is_js_whitespace)
                } else {
                    l.trim_start_matches(is_js_whitespace)
                }
            })
            .collect();
        if self.opts.prose_wrap == ProseWrap::Preserve {
            return raw
                .iter()
                .map(|l| {
                    if l.is_empty() {
                        vec![]
                    } else {
                        vec![l.to_string()]
                    }
                })
                .collect();
        }
        let mut reduced: Vec<Vec<String>> = Vec::new();
        for (i, l) in raw.iter().enumerate() {
            let words = if l.is_empty() {
                vec![]
            } else {
                split_with_single_space(l)
            };
            let merge = i != 0
                && !raw[i - 1].is_empty()
                && !words.is_empty()
                && !(ty == Y::QuoteDouble
                    && reduced
                        .last()
                        .and_then(|r| r.last())
                        .is_some_and(|w| w.ends_with('\\')));
            if merge && let Some(lastv) = reduced.last_mut() {
                lastv.extend(words);
            } else {
                reduced.push(words);
            }
        }
        if self.opts.prose_wrap == ProseWrap::Never {
            reduced = reduced.into_iter().map(|w| vec![w.join(" ")]).collect();
        }
        reduced
    }

    fn print_block(&mut self, id: usize, last: bool) -> R {
        let n = self.n(id).clone();
        let mut parent_indent = 0;
        let mut p = n.parent;
        while let Some(x) = p {
            if matches!(self.n(x).ty, Y::Sequence | Y::Mapping) {
                parent_indent += 1;
            }
            p = self.n(x).parent;
        }
        let mut parts = vec![s(if n.ty == Y::BlockFolded { ">" } else { "|" })];
        if let Some(ind) = n.indent {
            parts.push(s(ind.to_string()));
        }
        if n.chomping != Chomp::Clip {
            parts.push(s(if n.chomping == Chomp::Keep { "+" } else { "-" }));
        }
        if let Some(ic) = n.indicator {
            parts.push(s(" "));
            parts.push(self.print(ic, false)?);
        }
        let lines = self.block_lines(id, parent_indent, last);
        let mut contents = Vec::new();
        let count = lines.len();
        for (i, words) in lines.into_iter().enumerate() {
            if i == 0 {
                contents.push(hardline());
            }
            let empty_line = words.is_empty();
            contents.push(fill_words(words));
            if i != count - 1 {
                contents.push(if empty_line {
                    hardline()
                } else {
                    mark_as_root(literalline())
                });
            } else if n.chomping == Chomp::Keep && last {
                contents.push(dedent_to_root(if empty_line {
                    hardline()
                } else {
                    literalline()
                }));
            }
        }
        match n.indent {
            None => parts.push(dedent(align_spaces(self.opts.tab_width, concat(contents)))),
            Some(ind) => {
                let width = ind - 1 + parent_indent as i64;
                if width < 0 {
                    return bail("negative block indent");
                }
                parts.push(dedent_to_root(align_spaces(
                    width as usize,
                    concat(contents),
                )));
            }
        }
        Ok(concat(parts))
    }

    fn block_lines(&self, id: usize, parent_indent: usize, is_last: bool) -> Vec<Vec<String>> {
        let n = self.n(id);
        let content = if n.start.line == n.end.line {
            ""
        } else {
            let raw = self.slice(n.start.offset, n.end.offset);
            match raw.find('\n') {
                Some(i) => &raw[i + 1..],
                None => "",
            }
        };
        let leading: usize = match n.indent {
            None => {
                let mut found = usize::MAX;
                for l in content.split('\n') {
                    let spaces = l.bytes().take_while(|&b| b == b' ').count();
                    if l.as_bytes().get(spaces).is_some_and(|&b| b != b'\r') {
                        found = spaces;
                        break;
                    }
                }
                found
            }
            Some(ind) => (ind - 1 + parent_indent as i64).max(0) as usize,
        };
        let raw: Vec<&str> = content
            .split('\n')
            .map(|l| js_slice_from(l, leading))
            .collect();
        let lines: Vec<Vec<String>> =
            if self.opts.prose_wrap == ProseWrap::Preserve || n.ty == Y::BlockLiteral {
                raw.iter()
                    .map(|l| {
                        if l.is_empty() {
                            vec![]
                        } else {
                            vec![l.to_string()]
                        }
                    })
                    .collect()
            } else {
                let mut reduced: Vec<Vec<String>> = Vec::new();
                for (i, l) in raw.iter().enumerate() {
                    let words = if l.is_empty() {
                        vec![]
                    } else {
                        split_with_single_space(l)
                    };
                    let starts_ws = words
                        .first()
                        .and_then(|w| w.chars().next())
                        .is_some_and(is_js_whitespace);
                    let merge = i != 0
                        && !raw[i - 1].is_empty()
                        && !words.is_empty()
                        && !starts_ws
                        && !reduced.last().is_some_and(|r| {
                            let joined = r.join(",");
                            joined.chars().next().is_some_and(is_js_whitespace)
                                || joined.chars().last().is_some_and(is_js_whitespace)
                        });
                    if merge && let Some(lastv) = reduced.last_mut() {
                        lastv.extend(words);
                    } else {
                        reduced.push(words);
                    }
                }
                let mut out: Vec<Vec<String>> = reduced
                    .into_iter()
                    .map(|words| {
                        let mut acc: Vec<String> = Vec::new();
                        for w in words {
                            if let Some(prev) = acc.last_mut()
                                && prev.chars().last().is_some_and(is_js_whitespace)
                            {
                                prev.push(' ');
                                prev.push_str(&w);
                            } else {
                                acc.push(w);
                            }
                        }
                        acc
                    })
                    .collect();
                if self.opts.prose_wrap == ProseWrap::Never {
                    out = out.into_iter().map(|w| vec![w.join(" ")]).collect();
                }
                out
            };
        let mut lines = lines;
        if n.chomping == Chomp::Keep {
            if lines.last().is_some_and(Vec::is_empty) {
                lines.pop();
            }
            return lines;
        }
        let trailing = lines.iter().rev().take_while(|l| l.is_empty()).count();
        if trailing == 0 {
            lines
        } else if trailing >= 2 && !is_last {
            lines.truncate(lines.len() - (trailing - 1));
            lines
        } else {
            lines.truncate(lines.len() - trailing);
            lines
        }
    }

    fn absolutely_single_line(&self, id: Option<usize>) -> bool {
        let Some(id) = id else {
            return true;
        };
        let n = self.n(id);
        match n.ty {
            Y::Plain | Y::QuoteSingle | Y::QuoteDouble => {}
            Y::Alias => return true,
            _ => return false,
        }
        if self.opts.prose_wrap == ProseWrap::Preserve {
            return n.start.line == n.end.line;
        }
        let raw = self.slice(n.start.offset, n.end.offset);
        if raw.split('\n').any(|l| l.ends_with('\\')) {
            return false;
        }
        let value = match n.ty {
            Y::Plain => n.value.clone(),
            _ => self.quote_value(id),
        };
        match self.opts.prose_wrap {
            ProseWrap::Never => !value.contains('\n'),
            _ => !value.contains('\n') && !value.contains(' '),
        }
    }

    fn quote_value(&self, id: usize) -> String {
        self.n(id).value.clone()
    }

    fn is_single_line(&self, id: Option<usize>) -> bool {
        let Some(id) = id else {
            return true;
        };
        let n = self.n(id);
        match n.ty {
            Y::Plain | Y::QuoteDouble | Y::QuoteSingle => n.start.line == n.end.line,
            Y::Alias => true,
            _ => false,
        }
    }

    fn print_mapping_item(&mut self, id: usize, last: bool) -> R {
        let n = self.n(id).clone();
        let (key, value) = (n.kids[0], n.kids[1]);
        let parent = n.parent;
        let empty_key = self.is_empty_node(key);
        let empty_value = self.is_empty_node(value);
        if empty_key && empty_value {
            return Ok(s(": "));
        }
        let printed_key = self.print(key, last)?;
        let key_content = self.content(key);
        let space = if key_content.is_some_and(|k| self.n(k).ty == Y::Alias) {
            " "
        } else {
            ""
        };
        if empty_value {
            if n.ty == Y::FlowMappingItem && parent.is_some_and(|p| self.n(p).ty == Y::FlowMapping)
            {
                return Ok(printed_key);
            }
            let parent_is_set = parent
                .and_then(|p| self.n(p).tag)
                .is_some_and(|t| self.n(t).value == "tag:yaml.org,2002:set");
            if n.ty == Y::MappingItem
                && self.absolutely_single_line(key_content)
                && !key_content.is_some_and(|k| self.n(k).trailing.is_some())
                && !parent_is_set
            {
                return Ok(concat(vec![printed_key, s(space), s(":")]));
            }
            return Ok(concat(vec![s("? "), align_spaces(2, printed_key)]));
        }
        let printed_value = shared(self.print(value, last)?);
        if empty_key {
            return Ok(concat(vec![s(": "), align_spaces(2, printed_value)]));
        }
        let vn = self.n(value).clone();
        let kn = self.n(key).clone();
        if !vn.leading.is_empty() || !self.is_inline(key_content) {
            let mut parts = vec![s("? "), align_spaces(2, printed_key), hardline()];
            for &c in &vn.leading {
                parts.push(self.print(c, false)?);
                parts.push(hardline());
            }
            parts.push(s(": "));
            parts.push(align_spaces(2, printed_value));
            return Ok(concat(parts));
        }
        let value_content = self.content(value);
        let kc = key_content.map(|k| self.n(k).clone());
        let vc = value_content.map(|v| self.n(v).clone());
        let has_lead = |x: &Option<ast::U>| x.as_ref().is_some_and(|u| !u.leading.is_empty());
        let has_mid = |x: &Option<ast::U>| x.as_ref().is_some_and(|u| !u.middle.is_empty());
        let has_trail = |x: &Option<ast::U>| x.as_ref().is_some_and(|u| u.trailing.is_some());
        if self.is_single_line(key_content)
            && !has_lead(&kc)
            && !has_mid(&kc)
            && !has_trail(&kc)
            && kn.end_comments.is_empty()
            && !has_lead(&vc)
            && !has_mid(&vc)
            && vn.end_comments.is_empty()
            && self.absolutely_single_line(value_content)
        {
            return Ok(concat(vec![printed_key, s(space), s(": "), printed_value]));
        }
        self.ids += 1;
        let gid = format!("yaml-mapping-key-{}", self.ids);
        let grouped_key = group(concat(vec![
            if_break(s("? "), empty()),
            group_with(
                align_spaces(2, printed_key.clone()),
                false,
                Some(gid.clone()),
            ),
        ]));
        let explicit = concat(vec![
            hardline(),
            s(": "),
            align_spaces(2, printed_value.clone()),
        ]);
        let mut implicit = vec![s(space), s(":")];
        let vc_ty = vc.as_ref().map(|u| u.ty);
        if !vn.end_comments.is_empty()
            && let Some(u) = &vc
            && matches!(u.ty, Y::FlowMapping | Y::FlowSequence)
            && u.kids.is_empty()
        {
            implicit.push(s(" "));
        } else if has_lead(&vc)
            || (!vn.end_comments.is_empty()
                && vc.is_some()
                && !matches!(vc_ty, Some(Y::Mapping | Y::Sequence)))
            || (parent.is_some_and(|p| self.n(p).ty == Y::Mapping)
                && has_trail(&kc)
                && self.is_inline(value_content))
            || (matches!(vc_ty, Some(Y::Mapping | Y::Sequence))
                && vc
                    .as_ref()
                    .is_some_and(|u| u.tag.is_none() && u.anchor.is_none()))
        {
            implicit.push(hardline());
        } else if vc.is_some() {
            implicit.push(line());
        } else if vn.trailing.is_some() {
            implicit.push(s(" "));
        }
        implicit.push(printed_value);
        let implicit = align_spaces(self.opts.tab_width, concat(implicit));
        if self.absolutely_single_line(key_content)
            && !has_lead(&kc)
            && !has_mid(&kc)
            && kn.end_comments.is_empty()
        {
            return Ok(conditional_group(
                vec![concat(vec![printed_key, implicit])],
                false,
            ));
        }
        Ok(conditional_group(
            vec![concat(vec![
                grouped_key,
                if_break_id(explicit, implicit, Some(gid)),
            ])],
            false,
        ))
    }

    fn print_flow(&mut self, id: usize, last: bool) -> R {
        let n = self.n(id).clone();
        let is_map = n.ty == Y::FlowMapping;
        let (open, close) = if is_map { ("{", "}") } else { ("[", "]") };
        let spacing = if is_map && !n.kids.is_empty() && self.opts.bracket_spacing {
            line()
        } else {
            softline()
        };
        let last_empty_item = n.kids.last().is_some_and(|&l| {
            let ln = self.n(l);
            ln.ty == Y::FlowMappingItem
                && self.is_empty_node(ln.kids[0])
                && self.is_empty_node(ln.kids[1])
        });
        let len = n.kids.len();
        let mut children = Vec::new();
        for (i, &k) in n.kids.iter().enumerate() {
            let printed = self.print(k, last && i + 1 == len)?;
            if i + 1 == len {
                children.push(printed);
            } else {
                let next = n.kids[i + 1];
                let gap = if self.n(k).start.line != self.n(next).start.line {
                    self.next_empty_line(k)
                } else {
                    empty()
                };
                children.push(concat(vec![printed, s(","), line(), gap]));
            }
        }
        let mut inner = vec![spacing.clone(), concat(children)];
        inner.push(if self.opts.trailing_comma == TrailingComma::None {
            empty()
        } else {
            if_break(s(","), empty())
        });
        if !n.end_comments.is_empty() {
            let mut cs = Vec::new();
            for &c in &n.end_comments {
                cs.push(self.print(c, false)?);
            }
            inner.push(concat(vec![hardline(), join(&hardline(), cs)]));
        }
        Ok(concat(vec![
            s(open),
            align_spaces(self.opts.tab_width, concat(inner)),
            if last_empty_item { empty() } else { spacing },
            s(close),
        ]))
    }
}

fn embedded_json_name(filepath: &str) -> bool {
    let base = filepath.rsplit(['/', '\\']).next().unwrap_or(filepath);
    matches!(base, ".prettierrc" | ".stylelintrc" | ".lintstagedrc")
}

/// Prettier's `yaml` parser and printer: the doc for `text`, or why this port cannot print it.
pub fn print(text: &str, opts: &Options) -> R {
    if embedded_json_name(&opts.filepath) {
        return bail("yaml config file printed as json");
    }
    let Some(tree) = ast::parse(text) else {
        return bail("yaml parse error");
    };
    let mut p = P {
        t: &tree,
        text,
        opts,
        printed_lines: Default::default(),
        ids: 0,
    };
    p.print(tree.root, true)
}
