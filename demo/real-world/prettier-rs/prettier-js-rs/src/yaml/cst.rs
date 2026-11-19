//! yaml 1.10.2's CST parser (`dist/parse-cst.js`, `PlainValue-*.js`): the tree yaml-unist-parser
//! reads. yaml is ISC licensed, Copyright Eemeli Aro. Any error the parser records makes
//! `has_error` true; the caller then leaves the file to Prettier.

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum T {
    Alias,
    BlankLine,
    BlockFolded,
    BlockLiteral,
    Comment,
    Directive,
    Document,
    FlowMap,
    FlowSeq,
    Map,
    MapKey,
    MapValue,
    Plain,
    QuoteDouble,
    QuoteSingle,
    Seq,
    SeqItem,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Chomp {
    Clip,
    Keep,
    Strip,
}

#[derive(Clone, Copy, Debug)]
pub enum It {
    N(usize),
    Ch(u8, usize),
}

#[derive(Clone, Debug, Default)]
pub struct Ctx {
    pub at_line_start: bool,
    pub in_collection: bool,
    pub in_flow: bool,
    pub indent: Option<i64>,
    pub line_start: Option<usize>,
    pub parent: Option<usize>,
}

#[derive(Default)]
pub struct Overlay {
    at_line_start: Option<bool>,
    in_collection: Option<bool>,
    in_flow: Option<bool>,
    indent: Option<i64>,
    line_start: Option<usize>,
    parent: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct C {
    pub ty: T,
    pub range: (usize, usize),
    pub vr: Option<(usize, usize)>,
    pub props: Vec<(usize, usize)>,
    pub ctx: Ctx,
    pub node: Option<usize>,
    pub items: Vec<It>,
    pub directives: Vec<usize>,
    pub contents: Vec<usize>,
    pub header: Option<(usize, usize)>,
    pub block_indent: Option<i64>,
    pub chomping: Chomp,
    pub name: String,
}

pub struct Cst<'a> {
    pub src: &'a [u8],
    pub n: Vec<C>,
    pub docs: Vec<usize>,
    pub has_error: bool,
}

pub fn is_empty_range(r: (usize, usize)) -> bool {
    r.1 == 0 || r.1 <= r.0
}

pub fn c(src: &[u8], i: usize) -> u8 {
    src.get(i).copied().unwrap_or(0)
}

fn cm(src: &[u8], i: usize) -> u8 {
    match i.checked_sub(1) {
        Some(j) => c(src, j),
        None => 0,
    }
}

pub fn at_document_boundary(src: &[u8], offset: usize, sep: Option<u8>) -> bool {
    let ch0 = c(src, offset);
    if ch0 == 0 {
        return true;
    }
    let prev = cm(src, offset);
    if prev != 0 && prev != b'\n' {
        return false;
    }
    match sep {
        Some(s) => {
            if ch0 != s {
                return false;
            }
        }
        None => {
            if ch0 != b'-' && ch0 != b'.' {
                return false;
            }
        }
    }
    if c(src, offset + 1) != ch0 || c(src, offset + 2) != ch0 {
        return false;
    }
    let ch3 = c(src, offset + 3);
    ch3 == 0 || ch3 == b'\n' || ch3 == b'\t' || ch3 == b' '
}

fn end_of_identifier(src: &[u8], mut offset: usize) -> usize {
    let mut ch = c(src, offset);
    let verbatim = ch == b'<';
    let not_ok: &[u8] = if verbatim { b"\n\t >" } else { b"\n\t []{}," };
    while ch != 0 && !not_ok.contains(&ch) {
        offset += 1;
        ch = c(src, offset);
    }
    if verbatim && ch == b'>' {
        offset += 1;
    }
    offset
}

pub fn end_of_indent(src: &[u8], mut offset: usize) -> usize {
    while c(src, offset) == b' ' {
        offset += 1;
    }
    offset
}

pub fn end_of_line(src: &[u8], mut offset: usize) -> usize {
    loop {
        let ch = c(src, offset);
        if ch == 0 || ch == b'\n' {
            return offset;
        }
        offset += 1;
    }
}

pub fn end_of_white_space(src: &[u8], mut offset: usize) -> usize {
    while matches!(c(src, offset), b' ' | b'\t') {
        offset += 1;
    }
    offset
}

fn start_of_line(src: &[u8], offset: usize) -> usize {
    match src[..offset.min(src.len())]
        .iter()
        .rposition(|&b| b == b'\n')
    {
        Some(p) => p + 1,
        None => 0,
    }
}

fn end_of_block_indent(src: &[u8], indent: i64, line_start: usize) -> Option<usize> {
    let in_end = end_of_indent(src, line_start);
    if in_end as i64 > line_start as i64 + indent {
        return Some(in_end);
    }
    let ws_end = end_of_white_space(src, in_end);
    let ch = c(src, ws_end);
    if ch == 0 || ch == b'\n' {
        return Some(ws_end);
    }
    None
}

pub fn at_blank(src: &[u8], offset: usize, end_as_blank: bool) -> bool {
    let ch = c(src, offset);
    ch == b'\n' || ch == b'\t' || ch == b' ' || (end_as_blank && ch == 0)
}

fn next_node_is_indented(ch: u8, indent_diff: i64, indicator_as_indent: bool) -> bool {
    if ch == 0 || indent_diff < 0 {
        return false;
    }
    if indent_diff > 0 {
        return true;
    }
    indicator_as_indent && ch == b'-'
}

fn normalize_offset(src: &[u8], offset: usize) -> usize {
    let ch = c(src, offset);
    if ch == 0 {
        offset
    } else if ch != b'\n' && cm(src, offset) == b'\n' {
        offset - 1
    } else {
        end_of_white_space(src, offset)
    }
}

/// `Node.foldNewline`: the folded text, the offset of the last character read, and whether the
/// next line is insufficiently indented.
pub fn fold_newline(src: &[u8], mut offset: usize, indent: i64) -> (String, usize, bool) {
    let mut in_count: i64 = 0;
    let mut error = false;
    let mut fold = String::new();
    let mut ch = c(src, offset + 1);
    while ch == b' ' || ch == b'\t' || ch == b'\n' {
        match ch {
            b'\n' => {
                in_count = 0;
                offset += 1;
                fold.push('\n');
            }
            b'\t' => {
                if in_count <= indent {
                    error = true;
                }
                offset = end_of_white_space(src, offset + 2) - 1;
            }
            _ => {
                in_count += 1;
                offset += 1;
            }
        }
        ch = c(src, offset + 1);
    }
    if fold.is_empty() {
        fold.push(' ');
    }
    if ch != 0 && in_count <= indent {
        error = true;
    }
    (fold, offset, error)
}

fn plain_end_of_line(src: &[u8], start: usize, in_flow: bool) -> usize {
    let mut ch = c(src, start);
    let mut offset = start;
    while ch != 0 && ch != b'\n' {
        if in_flow && matches!(ch, b'[' | b']' | b'{' | b'}' | b',') {
            break;
        }
        let next = c(src, offset + 1);
        if ch == b':'
            && (next == 0
                || next == b'\n'
                || next == b'\t'
                || next == b' '
                || (in_flow && next == b','))
        {
            break;
        }
        if (ch == b' ' || ch == b'\t') && next == b'#' {
            break;
        }
        offset += 1;
        ch = next;
    }
    offset
}

fn tag_date_like(s: &[u8]) -> bool {
    let mut i = 0;
    let word = |i: &mut usize| {
        let st = *i;
        while *i < s.len() && (s[*i].is_ascii_alphanumeric() || s[*i] == b'-') {
            *i += 1;
        }
        *i > st
    };
    if !word(&mut i) || s.get(i) != Some(&b'.') {
        return false;
    }
    i += 1;
    if !word(&mut i) || s.get(i) != Some(&b',') {
        return false;
    }
    i += 1;
    for _ in 0..4 {
        if !s.get(i).is_some_and(u8::is_ascii_digit) {
            return false;
        }
        i += 1;
    }
    for _ in 0..2 {
        if s.get(i) == Some(&b'-')
            && s.get(i + 1).is_some_and(u8::is_ascii_digit)
            && s.get(i + 2).is_some_and(u8::is_ascii_digit)
        {
            i += 3;
        } else {
            break;
        }
    }
    s.get(i) == Some(&b'/')
        && s.get(i + 1)
            .is_some_and(|b| !b.is_ascii_whitespace() && *b != 0x0b)
}

impl<'a> Cst<'a> {
    fn new_node(&mut self, ty: T, props: Vec<(usize, usize)>) -> usize {
        self.n.push(C {
            ty,
            range: (0, 0),
            vr: None,
            props,
            ctx: Ctx::default(),
            node: None,
            items: Vec::new(),
            directives: Vec::new(),
            contents: Vec::new(),
            header: None,
            block_indent: None,
            chomping: Chomp::Clip,
            name: String::new(),
        });
        self.n.len() - 1
    }

    fn ch(&self, i: usize) -> u8 {
        c(self.src, i)
    }

    fn err(&mut self) {
        self.has_error = true;
    }

    pub fn is_item(&self, id: usize) -> bool {
        matches!(self.n[id].ty, T::MapKey | T::MapValue | T::SeqItem)
    }

    pub fn includes_trailing_lines(&self, id: usize) -> bool {
        let n = &self.n[id];
        match n.ty {
            T::BlankLine => true,
            T::MapKey | T::MapValue | T::SeqItem => {
                n.node.is_some_and(|x| self.includes_trailing_lines(x))
            }
            T::Map | T::Seq => !n.items.is_empty(),
            T::BlockFolded | T::BlockLiteral => n.chomping == Chomp::Keep,
            _ => false,
        }
    }

    pub fn has_comment(&self, id: usize) -> bool {
        self.n[id].props.iter().any(|p| self.ch(p.0) == b'#')
    }

    pub fn has_props(&self, id: usize) -> bool {
        self.n[id].props.iter().any(|p| self.ch(p.0) != b'#')
    }

    fn blank_line(&mut self, start: usize) -> usize {
        let id = self.new_node(T::BlankLine, Vec::new());
        self.n[id].range = (start, start + 1);
        id
    }

    fn parse_comment(&mut self, id: usize, start: usize) -> usize {
        if self.ch(start) == b'#' {
            let end = end_of_line(self.src, start + 1);
            self.n[id].props.push((start, end));
            return end;
        }
        start
    }

    fn comment(&mut self, start: usize, ctx: Ctx) -> usize {
        let id = self.new_node(T::Comment, Vec::new());
        self.n[id].ctx = ctx;
        let offset = self.parse_comment(id, start);
        self.n[id].range = (start, offset);
        id
    }

    pub fn parse(src: &'a [u8]) -> Cst<'a> {
        let mut cst = Cst {
            src,
            n: Vec::new(),
            docs: Vec::new(),
            has_error: false,
        };
        let mut offset = 0;
        loop {
            let (doc, next) = cst.parse_document(offset);
            cst.docs.push(doc);
            offset = next;
            if offset >= src.len() {
                break;
            }
        }
        cst
    }

    fn parse_document(&mut self, start: usize) -> (usize, usize) {
        let id = self.new_node(T::Document, Vec::new());
        let mut offset = if self.src.get(start..start + 3) == Some(&[0xEF, 0xBB, 0xBF]) {
            start + 3
        } else {
            start
        };
        offset = self.parse_directives(id, offset);
        offset = self.parse_contents(id, offset);
        (id, offset)
    }

    fn start_comment_or_end_blank_line(&self, start: usize) -> usize {
        let offset = end_of_white_space(self.src, start);
        let ch = self.ch(offset);
        if ch == b'#' || ch == b'\n' {
            offset
        } else {
            start
        }
    }

    fn parse_directives(&mut self, doc: usize, start: usize) -> usize {
        let mut at_line_start = true;
        let mut has_directives = false;
        let mut offset = start;
        let mut directives = Vec::new();
        while !at_document_boundary(self.src, offset, Some(b'-')) {
            offset = self.start_comment_or_end_blank_line(offset);
            match self.ch(offset) {
                b'\n' => {
                    if at_line_start {
                        let b = self.blank_line(offset);
                        offset += 1;
                        if offset < self.src.len() {
                            directives.push(b);
                        }
                    } else {
                        offset += 1;
                        at_line_start = true;
                    }
                }
                b'#' => {
                    let cm = self.comment(offset, Ctx::default());
                    offset = self.n[cm].range.1;
                    directives.push(cm);
                    at_line_start = false;
                }
                b'%' => {
                    let d = self.new_node(T::Directive, Vec::new());
                    self.n[d].ctx.parent = Some(doc);
                    let mut o = offset + 1;
                    while !matches!(self.ch(o), 0 | b'\n' | b'\t' | b' ') {
                        o += 1;
                    }
                    self.n[d].name = String::from_utf8_lossy(&self.src[offset + 1..o]).into_owned();
                    let ps = o;
                    while !matches!(self.ch(o), 0 | b'\n' | b'#') {
                        o += 1;
                    }
                    self.n[d].vr = Some((ps, o));
                    o = self.parse_comment(d, o);
                    self.n[d].range = (offset, o);
                    offset = o;
                    directives.push(d);
                    has_directives = true;
                    at_line_start = false;
                }
                _ => {
                    if has_directives {
                        self.err();
                    } else if !directives.is_empty() {
                        self.n[doc].contents = std::mem::take(&mut directives);
                    }
                    self.n[doc].directives = directives;
                    return offset;
                }
            }
        }
        if self.ch(offset) != 0 {
            self.n[doc].directives = directives;
            return offset + 3;
        }
        if has_directives {
            self.err();
        } else if !directives.is_empty() {
            self.n[doc].contents = std::mem::take(&mut directives);
        }
        self.n[doc].directives = directives;
        offset
    }

    fn parse_contents(&mut self, doc: usize, start: usize) -> usize {
        let mut line_start = start;
        while cm(self.src, line_start) == b'-' {
            line_start -= 1;
        }
        let mut offset = end_of_white_space(self.src, start);
        let mut at_line_start = line_start == start;
        self.n[doc].vr = Some((offset, offset));
        let doc_ctx = Ctx::default();
        while !at_document_boundary(self.src, offset, Some(b'.')) {
            match self.ch(offset) {
                b'\n' => {
                    if at_line_start {
                        let b = self.blank_line(offset);
                        offset += 1;
                        if offset < self.src.len() {
                            self.n[doc].contents.push(b);
                        }
                    } else {
                        offset += 1;
                        at_line_start = true;
                    }
                    line_start = offset;
                }
                b'#' => {
                    let cm = self.comment(offset, Ctx::default());
                    offset = self.n[cm].range.1;
                    self.n[doc].contents.push(cm);
                    at_line_start = false;
                }
                _ => {
                    let i_end = end_of_indent(self.src, offset);
                    let overlay = Overlay {
                        at_line_start: Some(at_line_start),
                        indent: Some(-1),
                        in_flow: Some(false),
                        in_collection: Some(false),
                        line_start: Some(line_start),
                        parent: Some(doc),
                    };
                    let Some(node) = self.parse_node(&doc_ctx, overlay, i_end) else {
                        if let Some(vr) = &mut self.n[doc].vr {
                            vr.1 = i_end;
                        }
                        return i_end;
                    };
                    self.n[doc].contents.push(node);
                    offset = self.n[node].range.1;
                    at_line_start = false;
                    let ec = self.grab_collection_end_comments(node);
                    self.n[doc].contents.extend(ec);
                }
            }
            offset = self.start_comment_or_end_blank_line(offset);
        }
        if let Some(vr) = &mut self.n[doc].vr {
            vr.1 = offset;
        }
        if self.ch(offset) != 0 {
            offset += 3;
            if self.ch(offset) != 0 {
                offset = end_of_white_space(self.src, offset);
                if self.ch(offset) == b'#' {
                    let cm = self.comment(offset, Ctx::default());
                    offset = self.n[cm].range.1;
                    self.n[doc].contents.push(cm);
                }
                match self.ch(offset) {
                    b'\n' => offset += 1,
                    0 => {}
                    _ => self.err(),
                }
            }
        }
        offset
    }

    fn parse_props(&self, ctx: &mut Ctx, offset: usize) -> (Vec<(usize, usize)>, T, usize) {
        let src = self.src;
        let mut props = Vec::new();
        let mut line_has_props = false;
        let mut offset = if ctx.at_line_start {
            end_of_indent(src, offset)
        } else {
            end_of_white_space(src, offset)
        };
        let mut ch = c(src, offset);
        while matches!(ch, b'&' | b'#' | b'!' | b'\n') {
            if ch == b'\n' {
                let mut in_end = offset;
                let mut line_start;
                loop {
                    line_start = in_end + 1;
                    in_end = end_of_indent(src, line_start);
                    if c(src, in_end) != b'\n' {
                        break;
                    }
                }
                let indent_diff = in_end as i64 - (line_start as i64 + ctx.indent.unwrap_or(0));
                let no_indicator_as_indent = ctx
                    .parent
                    .is_some_and(|p| self.n[p].ty == T::SeqItem && self.n[p].ctx.at_line_start);
                if c(src, in_end) != b'#'
                    && !next_node_is_indented(c(src, in_end), indent_diff, !no_indicator_as_indent)
                {
                    break;
                }
                ctx.at_line_start = true;
                ctx.line_start = Some(line_start);
                line_has_props = false;
                offset = in_end;
            } else if ch == b'#' {
                let end = end_of_line(src, offset + 1);
                props.push((offset, end));
                offset = end;
            } else {
                let mut end = end_of_identifier(src, offset + 1);
                if ch == b'!'
                    && c(src, end) == b','
                    && tag_date_like(&src[(offset + 1).min(src.len())..(end + 13).min(src.len())])
                {
                    end = end_of_identifier(src, end + 5);
                }
                props.push((offset, end));
                line_has_props = true;
                offset = end_of_white_space(src, end);
            }
            ch = c(src, offset);
        }
        if line_has_props && ch == b':' && at_blank(src, offset + 1, true) {
            offset -= 1;
        }
        let in_flow = ctx.in_flow;
        let ty = match c(src, offset) {
            b'*' => T::Alias,
            b'>' => T::BlockFolded,
            b'|' => T::BlockLiteral,
            b'{' => T::FlowMap,
            b'[' => T::FlowSeq,
            b'?' if !in_flow && at_blank(src, offset + 1, true) => T::MapKey,
            b':' if !in_flow && at_blank(src, offset + 1, true) => T::MapValue,
            b'-' if !in_flow && at_blank(src, offset + 1, true) => T::SeqItem,
            b'"' => T::QuoteDouble,
            b'\'' => T::QuoteSingle,
            _ => T::Plain,
        };
        (props, ty, offset)
    }

    fn parse_node(&mut self, this: &Ctx, o: Overlay, start: usize) -> Option<usize> {
        if at_document_boundary(self.src, start, None) {
            return None;
        }
        let mut ctx = Ctx {
            at_line_start: o.at_line_start.unwrap_or(this.at_line_start),
            in_collection: o.in_collection.unwrap_or(this.in_collection),
            in_flow: o.in_flow.unwrap_or(this.in_flow),
            indent: o.indent.or(this.indent),
            line_start: o.line_start.or(this.line_start),
            parent: o.parent.or(this.parent),
        };
        let (props, ty, value_start) = self.parse_props(&mut ctx, start);
        let id = self.new_node(ty, props);
        self.n[id].ctx = ctx.clone();
        let mut offset = match ty {
            T::Alias => self.parse_alias(id, value_start),
            T::BlockFolded | T::BlockLiteral => self.parse_block(id, value_start),
            T::FlowMap | T::FlowSeq => self.parse_flow(id, value_start),
            T::MapKey | T::MapValue | T::SeqItem => self.parse_item(id, value_start),
            T::QuoteDouble | T::QuoteSingle => self.parse_quote(id, value_start),
            _ => self.parse_plain(id, value_start),
        };
        self.n[id].range = (start, offset);
        if offset <= start {
            self.err();
            self.n[id].range.1 = start + 1;
        }
        if self.node_starts_collection(&ctx, id) {
            if !ctx.at_line_start && ctx.parent.is_some_and(|p| self.n[p].ty == T::Document) {
                self.err();
            }
            let coll = self.new_collection(id);
            offset = self.parse_collection(coll, ctx, offset);
            self.n[coll].range = (start, offset);
            return Some(coll);
        }
        Some(id)
    }

    fn node_starts_collection(&self, ctx: &Ctx, id: usize) -> bool {
        if ctx.in_collection || ctx.in_flow {
            return false;
        }
        if self.is_item(id) {
            return true;
        }
        let mut offset = self.n[id].range.1;
        if self.ch(offset) == b'\n' || cm(self.src, offset) == b'\n' {
            return false;
        }
        offset = end_of_white_space(self.src, offset);
        self.ch(offset) == b':'
    }

    fn parse_alias(&mut self, id: usize, start: usize) -> usize {
        let mut offset = end_of_identifier(self.src, start + 1);
        self.n[id].vr = Some((start + 1, offset));
        offset = end_of_white_space(self.src, offset);
        self.parse_comment(id, offset)
    }

    fn parse_plain(&mut self, id: usize, start: usize) -> usize {
        let in_flow = self.n[id].ctx.in_flow;
        let mut offset = start;
        let ch = self.ch(offset);
        if ch != 0 && ch != b'#' && ch != b'\n' {
            offset = plain_end_of_line(self.src, start, in_flow);
        }
        self.n[id].vr = Some((start, offset));
        offset = end_of_white_space(self.src, offset);
        offset = self.parse_comment(id, offset);
        if !self.has_comment(id) || is_empty_range(self.n[id].vr.unwrap_or((0, 0))) {
            offset = self.parse_plain_block_value(id, offset);
        }
        offset
    }

    fn parse_plain_block_value(&mut self, id: usize, start: usize) -> usize {
        let indent = self.n[id].ctx.indent.unwrap_or(0);
        let in_flow = self.n[id].ctx.in_flow;
        let src = self.src;
        let mut offset = start;
        let mut value_end = start;
        while c(src, offset) == b'\n' {
            if at_document_boundary(src, offset + 1, None) {
                break;
            }
            let Some(end) = end_of_block_indent(src, indent, offset + 1) else {
                break;
            };
            if c(src, end) == b'#' {
                break;
            }
            if c(src, end) == b'\n' {
                offset = end;
            } else {
                value_end = plain_end_of_line(src, end, in_flow);
                offset = value_end;
            }
        }
        let vr = self.n[id].vr.get_or_insert((start, start));
        if is_empty_range(*vr) {
            vr.0 = start;
        }
        vr.1 = value_end;
        value_end
    }

    fn parse_block(&mut self, id: usize, start: usize) -> usize {
        let src = self.src;
        let mut offset = start + 1;
        let mut bi = String::new();
        loop {
            match c(src, offset) {
                b'-' => self.n[id].chomping = Chomp::Strip,
                b'+' => self.n[id].chomping = Chomp::Keep,
                d @ b'0'..=b'9' => bi.push(d as char),
                _ => {
                    let n: i64 = bi.parse().unwrap_or(0);
                    self.n[id].block_indent = if n == 0 { None } else { Some(n) };
                    self.n[id].header = Some((start, offset));
                    break;
                }
            }
            offset += 1;
        }
        offset = end_of_white_space(src, offset);
        offset = self.parse_comment(id, offset);
        self.parse_block_value(id, offset)
    }

    fn parse_block_value(&mut self, id: usize, start: usize) -> usize {
        let src = self.src;
        let indent = self.n[id].ctx.indent.unwrap_or(0);
        let mut offset = start;
        let mut value_end = start;
        let mut min_block_indent = 1;
        while c(src, offset) == b'\n' {
            offset += 1;
            if at_document_boundary(src, offset, None) {
                break;
            }
            let Some(end) = end_of_block_indent(src, indent, offset) else {
                break;
            };
            let ch = c(src, end);
            let line_indent = end as i64 - (offset as i64 + indent);
            match self.n[id].block_indent {
                None => {
                    if ch != b'\n' {
                        if line_indent < min_block_indent {
                            self.err();
                        }
                        self.n[id].block_indent = Some(line_indent);
                    } else if line_indent > min_block_indent {
                        min_block_indent = line_indent;
                    }
                }
                Some(bi) => {
                    if ch != 0 && ch != b'\n' && line_indent < bi {
                        if ch == b'#' {
                            break;
                        }
                        self.err();
                    }
                }
            }
            if ch == b'\n' {
                offset = end;
            } else {
                value_end = end_of_line(src, end);
                offset = value_end;
            }
        }
        if self.n[id].chomping != Chomp::Keep {
            offset = if c(src, value_end) != 0 {
                value_end + 1
            } else {
                value_end
            };
        }
        self.n[id].vr = Some((start + 1, if offset == 0 { start + 1 } else { offset }));
        offset
    }

    fn parse_quote(&mut self, id: usize, start: usize) -> usize {
        let src = self.src;
        let mut offset = start + 1;
        if self.n[id].ty == T::QuoteDouble {
            loop {
                let ch = c(src, offset);
                if ch == 0 || ch == b'"' {
                    break;
                }
                offset += if ch == b'\\' { 2 } else { 1 };
            }
        } else {
            loop {
                let ch = c(src, offset);
                if ch == 0 {
                    break;
                }
                if ch == b'\'' {
                    if c(src, offset + 1) != b'\'' {
                        break;
                    }
                    offset += 2;
                } else {
                    offset += 1;
                }
            }
        }
        offset += 1;
        self.n[id].vr = Some((start, offset));
        offset = end_of_white_space(src, offset);
        self.parse_comment(id, offset)
    }

    fn parse_item(&mut self, id: usize, start: usize) -> usize {
        let src = self.src;
        let ctx = self.n[id].ctx.clone();
        let mut at_line_start = ctx.at_line_start;
        let mut line_start = ctx.line_start.unwrap_or(0);
        if !at_line_start && self.n[id].ty == T::SeqItem {
            self.err();
        }
        let indent = if at_line_start {
            start as i64 - line_start as i64
        } else {
            ctx.indent.unwrap_or(0)
        };
        let mut offset = end_of_white_space(src, start + 1);
        let mut ch = c(src, offset);
        let inline_comment = ch == b'#';
        let mut comments = Vec::new();
        let mut blank = None;
        while ch == b'\n' || ch == b'#' {
            if ch == b'#' {
                let end = end_of_line(src, offset + 1);
                comments.push((offset, end));
                offset = end;
            } else {
                at_line_start = true;
                line_start = offset + 1;
                let ws_end = end_of_white_space(src, line_start);
                if c(src, ws_end) == b'\n' && comments.is_empty() {
                    blank = Some(self.blank_line(line_start));
                    line_start += 1;
                }
                offset = end_of_indent(src, line_start);
            }
            ch = c(src, offset);
        }
        let mut node = None;
        if next_node_is_indented(
            ch,
            offset as i64 - (line_start as i64 + indent),
            self.n[id].ty != T::SeqItem,
        ) {
            let overlay = Overlay {
                at_line_start: Some(at_line_start),
                in_collection: Some(false),
                indent: Some(indent),
                line_start: Some(line_start),
                parent: Some(id),
                ..Default::default()
            };
            node = self.parse_node(&ctx, overlay, offset);
        }
        self.n[id].node = node;
        if let Some(nd) = node {
            if let Some(b) = blank
                && let Some(p) = ctx.parent
            {
                match self.n[p].ty {
                    T::Map | T::Seq => self.n[p].items.push(It::N(b)),
                    T::Document => self.n[p].contents.push(b),
                    _ => {}
                }
            }
            self.n[id].props.extend(comments);
            offset = self.n[nd].range.1;
        } else if inline_comment {
            let c0 = comments[0];
            self.n[id].props.push(c0);
            offset = c0.1;
        } else {
            offset = end_of_line(src, start + 1);
        }
        let end = match node {
            Some(nd) => self.n[nd].vr.map_or(offset, |v| v.1),
            None => offset,
        };
        self.n[id].vr = Some((start, end));
        offset
    }

    fn prev_node_is_json_like(&self, id: usize, idx: usize) -> bool {
        if idx == 0 {
            return false;
        }
        match self.n[id].items[idx - 1] {
            It::N(n) => match self.n[n].ty {
                T::FlowMap | T::FlowSeq | T::QuoteDouble | T::QuoteSingle => true,
                T::Comment => self.prev_node_is_json_like(id, idx - 1),
                _ => false,
            },
            It::Ch(..) => false,
        }
    }

    fn parse_flow(&mut self, id: usize, start: usize) -> usize {
        let src = self.src;
        let ctx = self.n[id].ctx.clone();
        let indent = ctx.indent.unwrap_or(0);
        let mut line_start = ctx.line_start.unwrap_or(0);
        self.n[id].items.push(It::Ch(c(src, start), start));
        let mut offset = end_of_white_space(src, start + 1);
        let mut ch = c(src, offset);
        while ch != 0 && ch != b']' && ch != b'}' {
            let mut as_node = false;
            match ch {
                b'\n' => {
                    line_start = offset + 1;
                    let ws_end = end_of_white_space(src, line_start);
                    if c(src, ws_end) == b'\n' {
                        let b = self.blank_line(line_start);
                        line_start += 1;
                        self.n[id].items.push(It::N(b));
                    }
                    offset = end_of_indent(src, line_start);
                    if offset as i64 <= line_start as i64 + indent {
                        let c2 = c(src, offset);
                        if (offset as i64) < line_start as i64 + indent
                            || (c2 != b']' && c2 != b'}')
                        {
                            self.err();
                        }
                    }
                }
                b',' => {
                    self.n[id].items.push(It::Ch(b',', offset));
                    offset += 1;
                }
                b'#' => {
                    let cmt = self.comment(offset, Ctx::default());
                    offset = self.n[cmt].range.1;
                    self.n[id].items.push(It::N(cmt));
                }
                b'?' | b':' => {
                    let next = c(src, offset + 1);
                    let len = self.n[id].items.len();
                    if matches!(next, b'\n' | b'\t' | b' ' | b',')
                        || (ch == b':' && self.prev_node_is_json_like(id, len))
                    {
                        self.n[id].items.push(It::Ch(ch, offset));
                        offset += 1;
                    } else {
                        as_node = true;
                    }
                }
                _ => as_node = true,
            }
            if as_node {
                let overlay = Overlay {
                    at_line_start: Some(false),
                    in_collection: Some(false),
                    in_flow: Some(true),
                    indent: Some(-1),
                    line_start: Some(line_start),
                    parent: Some(id),
                };
                let Some(node) = self.parse_node(&ctx, overlay, offset) else {
                    self.n[id].vr = Some((start, offset));
                    return offset;
                };
                self.n[id].items.push(It::N(node));
                offset = normalize_offset(src, self.n[node].range.1);
            }
            offset = end_of_white_space(src, offset);
            ch = c(src, offset);
        }
        self.n[id].vr = Some((start, offset + 1));
        if ch != 0 {
            self.n[id].items.push(It::Ch(ch, offset));
            offset = end_of_white_space(src, offset + 1);
            offset = self.parse_comment(id, offset);
        }
        offset
    }

    fn new_collection(&mut self, first: usize) -> usize {
        let ty = if self.n[first].ty == T::SeqItem {
            T::Seq
        } else {
            T::Map
        };
        let id = self.new_node(ty, Vec::new());
        let ls = self.n[first].ctx.line_start.unwrap_or(0);
        let props = self.n[first].props.clone();
        for i in (0..props.len()).rev() {
            if props[i].0 < ls {
                self.n[id].props = props[..=i].to_vec();
                self.n[first].props = props[i + 1..].to_vec();
                let item_start = match self.n[first].props.first() {
                    Some(p) => p.0,
                    None => self.n[first].vr.map_or(0, |v| v.0),
                };
                self.n[first].range.0 = item_start;
                break;
            }
        }
        self.n[id].items = vec![It::N(first)];
        let ec = self.grab_collection_end_comments(first);
        self.n[id].items.extend(ec.into_iter().map(It::N));
        id
    }

    fn grab_collection_end_comments(&mut self, node: usize) -> Vec<usize> {
        let mut cnode = Some(node);
        while let Some(x) = cnode
            && self.is_item(x)
        {
            cnode = self.n[x].node;
        }
        let Some(cn) = cnode else {
            return Vec::new();
        };
        if !matches!(self.n[cn].ty, T::Map | T::Seq) {
            return Vec::new();
        }
        let len = self.n[cn].items.len();
        let mut ci = None;
        for i in (0..len).rev() {
            let It::N(n) = self.n[cn].items[i] else {
                break;
            };
            match self.n[n].ty {
                T::Comment => {
                    let ctx = &self.n[n].ctx;
                    if let (Some(indent), Some(ls)) = (ctx.indent, ctx.line_start)
                        && indent > 0
                        && self.n[n].range.0 as i64 >= ls as i64 + indent
                    {
                        break;
                    }
                    ci = Some(i);
                }
                T::BlankLine => ci = Some(i),
                _ => break,
            }
        }
        let Some(ci) = ci else {
            return Vec::new();
        };
        let ca: Vec<usize> = self.n[cn]
            .items
            .drain(ci..)
            .filter_map(|it| match it {
                It::N(n) => Some(n),
                It::Ch(..) => None,
            })
            .collect();
        let prev_end = self.n[ca[0]].range.0;
        let mut cur = cn;
        loop {
            self.n[cur].range.1 = prev_end;
            if let Some(vr) = &mut self.n[cur].vr
                && vr.1 > prev_end
            {
                vr.1 = prev_end;
            }
            if cur == node {
                break;
            }
            match self.n[cur].ctx.parent {
                Some(p) => cur = p,
                None => break,
            }
        }
        ca
    }

    fn next_content_has_indent(&self, offset: usize, indent: i64) -> bool {
        let line_start = end_of_line(self.src, offset) + 1;
        let offset = end_of_white_space(self.src, line_start);
        let ch = self.ch(offset);
        if ch == 0 {
            return false;
        }
        if offset as i64 >= line_start as i64 + indent {
            return true;
        }
        if ch != b'#' && ch != b'\n' {
            return false;
        }
        self.next_content_has_indent(offset, indent)
    }

    fn parse_collection(&mut self, id: usize, ctx: Ctx, start: usize) -> usize {
        let src = self.src;
        self.n[id].ctx = ctx.clone();
        let mut line_start = start_of_line(src, start);
        let It::N(first) = self.n[id].items[0] else {
            return start;
        };
        self.n[first].ctx.parent = Some(id);
        self.n[id].vr = self.n[first].vr;
        let indent =
            self.n[first].range.0 as i64 - self.n[first].ctx.line_start.unwrap_or(0) as i64;
        let first_is_seq = self.n[first].ty == T::SeqItem;
        let mut offset = normalize_offset(src, start);
        let mut ch = c(src, offset);
        let mut at_line_start = end_of_white_space(src, line_start) == offset;
        let mut prev_includes_trailing = false;
        while ch != 0 {
            while ch == b'\n' || ch == b'#' {
                if at_line_start && ch == b'\n' && !prev_includes_trailing {
                    let b = self.blank_line(offset);
                    offset += 1;
                    if let Some(vr) = &mut self.n[id].vr {
                        vr.1 = offset;
                    }
                    if offset >= src.len() {
                        ch = 0;
                        break;
                    }
                    self.n[id].items.push(It::N(b));
                    offset -= 1;
                } else if ch == b'#' {
                    if (offset as i64) < line_start as i64 + indent
                        && !self.next_content_has_indent(offset, indent)
                    {
                        return offset;
                    }
                    let cctx = Ctx {
                        indent: Some(indent),
                        line_start: Some(line_start),
                        ..Default::default()
                    };
                    let cmt = self.comment(offset, cctx);
                    offset = self.n[cmt].range.1;
                    self.n[id].items.push(It::N(cmt));
                    if let Some(vr) = &mut self.n[id].vr {
                        vr.1 = offset;
                    }
                    if offset >= src.len() {
                        ch = 0;
                        break;
                    }
                }
                line_start = offset + 1;
                offset = end_of_indent(src, line_start);
                if at_blank(src, offset, false) {
                    let ws_end = end_of_white_space(src, offset);
                    let next = c(src, ws_end);
                    if next == 0 || next == b'\n' || next == b'#' {
                        offset = ws_end;
                    }
                }
                ch = c(src, offset);
                at_line_start = true;
            }
            if ch == 0 {
                break;
            }
            if offset as i64 != line_start as i64 + indent && (at_line_start || ch != b':') {
                if (offset as i64) < line_start as i64 + indent {
                    if line_start > start {
                        offset = line_start;
                    }
                    break;
                }
                self.err();
            }
            if first_is_seq {
                if ch != b'-' {
                    if line_start > start {
                        offset = line_start;
                    }
                    break;
                }
            } else if ch == b'-' {
                let next = c(src, offset + 1);
                if matches!(next, 0 | b'\n' | b'\t' | b' ') {
                    self.err();
                }
            }
            let overlay = Overlay {
                at_line_start: Some(at_line_start),
                in_collection: Some(true),
                indent: Some(indent),
                line_start: Some(line_start),
                parent: Some(id),
                ..Default::default()
            };
            let Some(node) = self.parse_node(&ctx, overlay, offset) else {
                return offset;
            };
            self.n[id].items.push(It::N(node));
            let node_vr_end = self.n[node].vr.map_or(0, |v| v.1);
            if let Some(vr) = &mut self.n[id].vr {
                vr.1 = node_vr_end;
            }
            offset = normalize_offset(src, self.n[node].range.1);
            ch = c(src, offset);
            at_line_start = false;
            prev_includes_trailing = self.includes_trailing_lines(node);
            if ch != 0 {
                let mut ls = offset as i64 - 1;
                let mut prev = if ls >= 0 { c(src, ls as usize) } else { 0 };
                while prev == b' ' || prev == b'\t' {
                    ls -= 1;
                    prev = if ls >= 0 { c(src, ls as usize) } else { 0 };
                }
                if prev == b'\n' {
                    line_start = (ls + 1) as usize;
                    at_line_start = true;
                }
            }
            let ec = self.grab_collection_end_comments(node);
            self.n[id].items.extend(ec.into_iter().map(It::N));
        }
        offset
    }
}
