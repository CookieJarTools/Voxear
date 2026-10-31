//! The Markdown VoxDocs reads: CommonMark's everyday parts (headings,
//! paragraphs, emphasis, code, links, lists, quotes, rules), plus tables,
//! checklists, bare web links, `~~strikethrough~~` and `==highlights==`,
//! GitHub-style callouts (`> [!NOTE]`), heading anchors, and `[[Page]]` and
//! `[[Page|words]]` links, as the CookieJarTools Cookbook reads it.
//!
//! Paragraphs come out as [`RichText`], ready to draw. A `[[Page]]` link
//! becomes a link to `docs:Page`; other links keep their address.

use voxear_ui::{Marks, RichText};

/// Where `[[Page]]` links point: `docs:` and the page's title, id or path.
pub const PAGE_SCHEME: &str = "docs:";

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    /// `level` is 1 to 6; `anchor` is what `#anchor` links to it with.
    Heading {
        level: u8,
        text: RichText,
        anchor: String,
    },
    Paragraph(RichText),
    /// A numbered list starts at `start`; a bulleted one has none.
    List {
        start: Option<u64>,
        items: Vec<Item>,
    },
    /// A quote, or a callout when it starts with `[!NOTE]` and the like.
    Quote {
        callout: Option<Callout>,
        blocks: Vec<Block>,
    },
    Code {
        language: String,
        code: String,
    },
    Table {
        align: Vec<Align>,
        head: Vec<RichText>,
        rows: Vec<Vec<RichText>>,
    },
    /// A picture on a line of its own.
    Image {
        alt: String,
        url: String,
    },
    Rule,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// For a checklist item: whether it is ticked.
    pub task: Option<Task>,
    pub blocks: Vec<Block>,
}

/// A checklist box, and where its mark (the space or `x` between the
/// brackets) is in the Markdown, so ticking it can change the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Task {
    pub done: bool,
    pub at: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Callout {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

impl Callout {
    pub fn title(self) -> &'static str {
        match self {
            Callout::Note => "Note",
            Callout::Tip => "Tip",
            Callout::Important => "Important",
            Callout::Warning => "Warning",
            Callout::Caution => "Caution",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        Some(match text.to_ascii_uppercase().as_str() {
            "NOTE" => Callout::Note,
            "TIP" => Callout::Tip,
            "IMPORTANT" => Callout::Important,
            "WARNING" => Callout::Warning,
            "CAUTION" => Callout::Caution,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    None,
    Left,
    Center,
    Right,
}

/// Reads a page's Markdown, its header already taken off.
pub fn parse(markdown: &str) -> Vec<Block> {
    let mut at = 0;
    let lines: Vec<Line> = markdown
        .split_inclusive('\n')
        .map(|raw| {
            let line = Line { text: raw.trim_end_matches(['\n', '\r']), at };
            at += raw.len();
            line
        })
        .collect();
    let mut anchors = Anchors::default();
    blocks(&lines, &mut anchors)
}

/// The words of a page with the Markdown taken out, for searching.
pub fn plain_text(blocks: &[Block]) -> String {
    let mut out = String::new();
    for block in blocks {
        match block {
            Block::Heading { text, .. } | Block::Paragraph(text) => out.push_str(&text.text),
            Block::List { items, .. } => {
                out.push_str(&items.iter().map(|i| plain_text(&i.blocks)).collect::<Vec<_>>().join(" "))
            }
            Block::Quote { blocks, .. } => out.push_str(&plain_text(blocks)),
            Block::Code { code, .. } => out.push_str(code),
            Block::Table { head, rows, .. } => {
                for cell in head.iter().chain(rows.iter().flatten()) {
                    out.push_str(&cell.text);
                    out.push(' ');
                }
            }
            Block::Image { alt, .. } => out.push_str(alt),
            Block::Rule => {}
        }
        out.push('\n');
    }
    out
}

/// What `#anchor` links a heading with: GitHub's way, lower case with
/// dashes for spaces and other punctuation dropped.
pub fn anchor(text: &str) -> String {
    text.trim()
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            _ => None,
        })
        .collect()
}

#[derive(Clone, Copy)]
struct Line<'a> {
    text: &'a str,
    /// Where it starts in the whole Markdown.
    at: usize,
}

impl<'a> Line<'a> {
    fn is_blank(&self) -> bool {
        self.text.trim().is_empty()
    }

    fn indent(&self) -> usize {
        self.text.len() - self.text.trim_start_matches(' ').len()
    }

    /// The line from byte `from`, or from where its spaces end if sooner.
    fn from(&self, from: usize) -> Line<'a> {
        let from = from.min(self.indent()).min(self.text.len());
        Line { text: &self.text[from..], at: self.at + from }
    }
}

/// Anchors given out so far, so two headings with the same words differ.
#[derive(Default)]
struct Anchors(Vec<String>);

impl Anchors {
    fn unique(&mut self, text: &str) -> String {
        let base = anchor(text);
        let mut candidate = base.clone();
        let mut count = 0;
        while self.0.contains(&candidate) {
            count += 1;
            candidate = format!("{base}-{count}");
        }
        self.0.push(candidate.clone());
        candidate
    }
}

fn blocks(lines: &[Line], anchors: &mut Anchors) -> Vec<Block> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.is_blank() {
            i += 1;
            continue;
        }
        let consumed = fence(&lines[i..], &mut out)
            .or_else(|| heading(line, anchors, &mut out))
            .or_else(|| rule(line, &mut out))
            .or_else(|| quote(&lines[i..], anchors, &mut out))
            .or_else(|| list(&lines[i..], anchors, &mut out))
            .or_else(|| table(&lines[i..], &mut out))
            .unwrap_or_else(|| paragraph(&lines[i..], anchors, &mut out));
        i += consumed.max(1);
    }
    out
}

/// A fenced code block: ``` or ~~~, then a language.
fn fence(lines: &[Line], out: &mut Vec<Block>) -> Option<usize> {
    let first = lines[0];
    if first.indent() > 3 {
        return None;
    }
    let opener = first.text.trim_start();
    let mark = opener.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let length = opener.chars().take_while(|c| *c == mark).count();
    if length < 3 {
        return None;
    }
    let language = opener[length..].split_whitespace().next().unwrap_or_default().to_string();
    let mut code = Vec::new();
    let mut end = lines.len();
    for (index, line) in lines.iter().enumerate().skip(1) {
        let trimmed = line.text.trim();
        if trimmed.len() >= length && trimmed.chars().all(|c| c == mark) {
            end = index + 1;
            break;
        }
        code.push(line.from(first.indent()).text);
    }
    out.push(Block::Code { language, code: code.join("\n") });
    Some(end.min(lines.len()))
}

fn heading(line: Line, anchors: &mut Anchors, out: &mut Vec<Block>) -> Option<usize> {
    if line.indent() > 3 {
        return None;
    }
    let text = line.text.trim_start();
    let level = text.chars().take_while(|c| *c == '#').count();
    if !(1..=6).contains(&level) || !text[level..].is_empty() && !text[level..].starts_with(' ') {
        return None;
    }
    let words = text[level..].trim().trim_end_matches('#').trim_end();
    let rich = inline(words);
    let anchor = anchors.unique(&rich.text);
    out.push(Block::Heading { level: level as u8, text: rich, anchor });
    Some(1)
}

fn is_rule(line: Line) -> bool {
    let text: String = line.text.chars().filter(|c| !c.is_whitespace()).collect();
    line.indent() <= 3
        && text.len() >= 3
        && text.chars().next().is_some_and(|first| matches!(first, '-' | '*' | '_') && text.chars().all(|c| c == first))
}

fn rule(line: Line, out: &mut Vec<Block>) -> Option<usize> {
    is_rule(line).then(|| {
        out.push(Block::Rule);
        1
    })
}

/// A quote: lines starting `>`. One starting `[!NOTE]` and the like is a callout.
fn quote(lines: &[Line], anchors: &mut Anchors, out: &mut Vec<Block>) -> Option<usize> {
    let inside = |line: &Line| line.indent() <= 3 && line.text.trim_start().starts_with('>');
    if !inside(&lines[0]) {
        return None;
    }
    let mut stripped = Vec::new();
    for line in lines.iter().take_while(|l| inside(l)) {
        let marker = line.indent() + 1;
        let rest = &line.text[marker..];
        let skip = marker + usize::from(rest.starts_with(' '));
        stripped.push(Line { text: &line.text[skip..], at: line.at + skip });
    }
    let count = stripped.len();
    let callout = stripped
        .first()
        .and_then(|l| l.text.trim().strip_prefix("[!"))
        .and_then(|rest| rest.strip_suffix(']'))
        .and_then(Callout::parse);
    let body = if callout.is_some() { &stripped[1..] } else { &stripped[..] };
    out.push(Block::Quote { callout, blocks: blocks(body, anchors) });
    Some(count)
}

/// A list marker at the start of a line: whether it is numbered (and from
/// what), and where the item's words start.
fn marker(line: Line) -> Option<(Option<u64>, usize)> {
    let indent = line.indent();
    if indent > 3 {
        return None;
    }
    let text = &line.text[indent..];
    let (number, width) = if text.starts_with(['-', '*', '+']) {
        (None, 1)
    } else {
        let digits = text.chars().take_while(char::is_ascii_digit).count();
        if digits == 0 || digits > 9 || !matches!(text[digits..].chars().next(), Some('.' | ')')) {
            return None;
        }
        (Some(text[..digits].parse().ok()?), digits + 1)
    };
    let after = &text[width..];
    if !after.is_empty() && !after.starts_with(' ') {
        return None;
    }
    let spaces = after.len() - after.trim_start_matches(' ').len();
    let spaces = if (1..=4).contains(&spaces) { spaces } else { 1.min(after.len()) };
    Some((number, indent + width + spaces))
}

fn list(lines: &[Line], anchors: &mut Anchors, out: &mut Vec<Block>) -> Option<usize> {
    if is_rule(lines[0]) {
        return None;
    }
    let (start, _) = marker(lines[0])?;
    let numbered = start.is_some();
    let base = lines[0].indent();
    let mut items = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let Some((number, content)) = marker(lines[i]).filter(|_| lines[i].indent() <= base + 1) else { break };
        if number.is_some() != numbered || is_rule(lines[i]) {
            break;
        }
        let mut body =
            vec![Line { text: &lines[i].text[content.min(lines[i].text.len())..], at: lines[i].at + content }];
        i += 1;
        while i < lines.len() {
            let line = lines[i];
            if line.is_blank() {
                // A blank line carries on the item only if what follows is indented into it.
                let next = lines[i + 1..].iter().find(|l| !l.is_blank());
                if next.is_some_and(|n| n.indent() >= content) {
                    body.push(line);
                    i += 1;
                    continue;
                }
                break;
            }
            if line.indent() >= content {
                body.push(line.from(content));
            } else if marker(line).is_some() || body.last().is_some_and(Line::is_blank) {
                break;
            } else {
                // A lazy continuation of the item's paragraph.
                body.push(line.from(line.indent()));
            }
            i += 1;
        }
        let task = task(&mut body[0]);
        items.push(Item { task, blocks: blocks(&body, anchors) });
    }
    out.push(Block::List { start, items });
    Some(i)
}

/// Takes `[ ] ` or `[x] ` off the start of an item.
fn task(first: &mut Line) -> Option<Task> {
    let text = first.text;
    let done = match text.get(..4) {
        Some("[ ] ") => false,
        Some("[x] " | "[X] ") => true,
        _ if text == "[ ]" => false,
        _ if text == "[x]" || text == "[X]" => true,
        _ => return None,
    };
    let at = first.at + 1;
    let skip = 4.min(text.len());
    *first = Line { text: &text[skip..], at: first.at + skip };
    Some(Task { done, at })
}

fn cells(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    let trimmed = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let trimmed = trimmed.strip_suffix('|').filter(|t| !t.ends_with('\\')).unwrap_or(trimmed);
    let mut cells = vec![String::new()];
    let mut chars = trimmed.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                cells.last_mut().expect("starts with one").push(chars.next().expect("peeked"))
            }
            '|' => cells.push(String::new()),
            c => cells.last_mut().expect("starts with one").push(c),
        }
    }
    cells.into_iter().map(|c| c.trim().to_string()).collect()
}

fn delimiter_row(line: &str) -> Option<Vec<Align>> {
    if !line.contains('-') {
        return None;
    }
    cells(line)
        .iter()
        .map(|cell| {
            let left = cell.starts_with(':');
            let right = cell.ends_with(':');
            let dashes = cell.trim_matches(':');
            (!dashes.is_empty() && dashes.chars().all(|c| c == '-')).then_some(match (left, right) {
                (true, true) => Align::Center,
                (true, false) => Align::Left,
                (false, true) => Align::Right,
                (false, false) => Align::None,
            })
        })
        .collect()
}

fn table(lines: &[Line], out: &mut Vec<Block>) -> Option<usize> {
    if lines.len() < 2 || !lines[0].text.contains('|') {
        return None;
    }
    let align = delimiter_row(lines[1].text)?;
    let head = cells(lines[0].text);
    if head.len() != align.len() {
        return None;
    }
    let mut rows = Vec::new();
    let mut i = 2;
    while i < lines.len() && !lines[i].is_blank() && lines[i].text.contains('|') {
        let mut row: Vec<RichText> = cells(lines[i].text).iter().map(|c| inline(c)).collect();
        row.resize(align.len(), RichText::new());
        rows.push(row);
        i += 1;
    }
    out.push(Block::Table { align, head: head.iter().map(|c| inline(c)).collect(), rows });
    Some(i)
}

/// Whether a line starts something other than more of a paragraph.
fn interrupts(line: Line) -> bool {
    let text = line.text.trim_start();
    line.indent() <= 3
        && (text.starts_with('#') && heading(line, &mut Anchors::default(), &mut Vec::new()).is_some()
            || text.starts_with("```")
            || text.starts_with("~~~")
            || text.starts_with('>')
            || is_rule(line)
            // An empty item, or a list not starting at 1, cannot interrupt a paragraph.
            || marker(line).is_some_and(|(number, content)| number.is_none_or(|n| n == 1) && content < line.text.len()))
}

fn paragraph(lines: &[Line], anchors: &mut Anchors, out: &mut Vec<Block>) -> usize {
    let mut text = String::new();
    let mut i = 0;
    while i < lines.len() && !lines[i].is_blank() && (i == 0 || !interrupts(lines[i])) {
        let line = lines[i].text;
        // `===` or `---` under a paragraph makes it a heading.
        let underline = line.trim();
        if i > 0
            && !underline.is_empty()
            && (underline.chars().all(|c| c == '=') || underline.chars().all(|c| c == '-'))
        {
            let level = if underline.starts_with('=') { 1 } else { 2 };
            let rich = inline(&text);
            let anchor = anchors.unique(&rich.text);
            out.push(Block::Heading { level, text: rich, anchor });
            return i + 1;
        }
        if i > 0
            && (i + 1 < lines.len() && lines[i + 1].text.contains('|') && delimiter_row(lines[i + 1].text).is_some())
        {
            break;
        }
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(line.trim_start());
        i += 1;
    }
    let trimmed = text.trim();
    if let Some((alt, url)) = lone_image(trimmed) {
        out.push(Block::Image { alt, url });
    } else {
        out.push(Block::Paragraph(inline(trimmed)));
    }
    i
}

/// `![alt](url)` and nothing else.
fn lone_image(text: &str) -> Option<(String, String)> {
    let rest = text.strip_prefix("![")?;
    let (alt, after, _) = bracketed(rest)?;
    let (url, used) = destination(after)?;
    (used == after.len()).then(|| (alt.to_string(), url))
}

/// The text up to the `]` that closes an opening bracket, what comes after
/// it, and how many bytes the inside took.
fn bracketed(text: &str) -> Option<(&str, &str, usize)> {
    let mut depth = 1;
    let mut chars = text.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '`' => {
                // Brackets inside code do not count.
                let run = text[i..].chars().take_while(|c| *c == '`').count();
                if let Some(close) = text[i + run..].find(&"`".repeat(run)) {
                    let to = i + run + close + run;
                    while chars.clone().next().is_some_and(|(j, _)| j < to) {
                        chars.next();
                    }
                }
            }
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some((&text[..i], &text[i + 1..], i));
                }
            }
            _ => {}
        }
    }
    None
}

/// `(url)` or `(<url with spaces> "title")` right at the start: the address
/// and how many bytes the whole took.
fn destination(text: &str) -> Option<(String, usize)> {
    let inner = text.strip_prefix('(')?;
    let mut depth = 1;
    for (i, c) in inner.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    let inside = inner[..i].trim();
                    let url = match inside.strip_prefix('<').and_then(|s| s.split_once('>')) {
                        Some((url, _)) => url.to_string(),
                        None => inside.split_whitespace().next().unwrap_or_default().to_string(),
                    };
                    return Some((url, i + 2));
                }
            }
            '\n' => return None,
            _ => {}
        }
    }
    None
}

/// A paragraph's inline Markdown as rich text. Single line breaks are
/// spaces; two spaces or a backslash at a line's end break it.
pub fn inline(markdown: &str) -> RichText {
    let mut joined = String::with_capacity(markdown.len());
    let lines: Vec<&str> = markdown.split('\n').collect();
    for (index, line) in lines.iter().enumerate() {
        let last = index + 1 == lines.len();
        if let Some(kept) = line.strip_suffix('\\').filter(|_| !last) {
            joined.push_str(kept);
            joined.push('\n');
        } else if line.ends_with("  ") && !last {
            joined.push_str(line.trim_end());
            joined.push('\n');
        } else {
            joined.push_str(if last { line } else { line.trim_end() });
            if !last {
                joined.push(' ');
            }
        }
    }
    let mut out = RichText::new();
    spans(&joined, Marks::default(), None, &mut out);
    out
}

/// The delimiters that mark words, longest first so `**` wins over `*`.
type Apply = fn(&mut Marks);

const DELIMITERS: &[(&str, Apply)] = &[
    ("***", |m| {
        m.strong = true;
        m.emphasis = true;
    }),
    ("**", |m| m.strong = true),
    ("__", |m| m.strong = true),
    ("~~", |m| m.strike = true),
    ("==", |m| m.mark = true),
    ("*", |m| m.emphasis = true),
    ("_", |m| m.emphasis = true),
];

fn push(out: &mut RichText, text: &str, marks: Marks, link: Option<&str>) {
    if text.is_empty() {
        return;
    }
    match link {
        Some(url) => {
            out.push_link(text, url, marks);
        }
        None => {
            out.push(text, marks);
        }
    }
}

/// Reads `text` into `out`, everything in it marked with `marks` and, if
/// given, a link to `link`.
fn spans(text: &str, marks: Marks, link: Option<&str>, out: &mut RichText) {
    let mut plain = String::new();
    let mut i = 0;
    let bytes = text.as_bytes();
    while i < text.len() {
        let rest = &text[i..];
        let c = rest.chars().next().expect("i is on a char");
        // An escaped punctuation mark is itself.
        if c == '\\'
            && let Some(next) = rest[1..].chars().next().filter(char::is_ascii_punctuation)
        {
            plain.push(next);
            i += 1 + next.len_utf8();
            continue;
        }
        if c == '`' {
            let run = rest.chars().take_while(|c| *c == '`').count();
            if let Some(close) = closing_backticks(&rest[run..], run) {
                push(out, &std::mem::take(&mut plain), marks, link);
                let code = &rest[run..run + close];
                let code = if code.len() > 2 && code.starts_with(' ') && code.ends_with(' ') {
                    &code[1..code.len() - 1]
                } else {
                    code
                };
                push(out, code, Marks { code: true, ..marks }, link);
                i += run + close + run;
                continue;
            }
            plain.push_str(&rest[..run]);
            i += run;
            continue;
        }
        if link.is_none()
            && rest.starts_with("[[")
            && let Some(close) = rest[2..]
                .find("]]")
                .filter(|&close| !rest[2..2 + close].contains('\n') && !rest[2..2 + close].trim().is_empty())
        {
            push(out, &std::mem::take(&mut plain), marks, link);
            let inner = &rest[2..2 + close];
            let (target, label) = inner.split_once('|').unwrap_or((inner, inner));
            let label = if label.trim().is_empty() { target } else { label };
            push(out, label.trim(), marks, Some(&format!("{PAGE_SCHEME}{}", target.trim())));
            i += close + 4;
            continue;
        }
        let image = rest.starts_with("![");
        if (image || c == '[')
            && link.is_none()
            && let Some((words, after, _)) = bracketed(&rest[1 + usize::from(image)..])
            && let Some((url, used)) = destination(after)
        {
            push(out, &std::mem::take(&mut plain), marks, link);
            if image {
                push(out, if words.is_empty() { &url } else { words }, marks, Some(&url));
            } else {
                spans(words, marks, Some(&url), out);
            }
            i += 1 + usize::from(image) + words.len() + 1 + used;
            continue;
        }
        if c == '<'
            && link.is_none()
            && let Some(close) = rest.find('>')
            && is_web(&rest[1..close])
            && !rest[1..close].contains(char::is_whitespace)
        {
            push(out, &std::mem::take(&mut plain), marks, link);
            let url = &rest[1..close];
            push(out, url, marks, Some(url));
            i += close + 1;
            continue;
        }
        let word_start = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
        if link.is_none() && word_start && is_web(rest) {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let url = rest[..end].trim_end_matches(['.', ',', ';', ':', '!', '?', ')', '"', '\'']);
            push(out, &std::mem::take(&mut plain), marks, link);
            let address = if url.starts_with("www.") { format!("https://{url}") } else { url.to_string() };
            push(out, url, marks, Some(&address));
            i += url.len();
            continue;
        }
        if let Some((delimiter, apply)) = DELIMITERS.iter().find(|(d, _)| rest.starts_with(d))
            && let Some(close) = closing(text, i, delimiter)
        {
            push(out, &std::mem::take(&mut plain), marks, link);
            let mut inner = marks;
            apply(&mut inner);
            spans(&text[i + delimiter.len()..close], inner, link, out);
            i = close + delimiter.len();
            continue;
        }
        plain.push(c);
        i += c.len_utf8();
    }
    push(out, &plain, marks, link);
}

fn is_web(text: &str) -> bool {
    ["https://", "http://", "mailto:", "www."].iter().any(|scheme| text.starts_with(scheme))
}

/// Where a run of `run` backticks closes a code span: the first run of
/// exactly that many.
fn closing_backticks(text: &str, run: usize) -> Option<usize> {
    let mut i = 0;
    while let Some(found) = text[i..].find('`') {
        let at = i + found;
        let length = text[at..].chars().take_while(|c| *c == '`').count();
        if length == run {
            return Some(at);
        }
        i = at + length;
    }
    None
}

/// Where the delimiter opening at `open` closes, as CommonMark mostly sees
/// it: the opener is followed by a non-space, the closer follows one, and
/// for `_` neither sits inside a word. Code spans in between are skipped.
fn closing(text: &str, open: usize, delimiter: &str) -> Option<usize> {
    let after = open + delimiter.len();
    let first = text[after..].chars().next()?;
    if first.is_whitespace() {
        return None;
    }
    let underscore = delimiter.starts_with('_');
    if underscore && open > 0 && text[..open].chars().next_back().is_some_and(char::is_alphanumeric) {
        return None;
    }
    let single = delimiter.len() == 1;
    let mark = delimiter.chars().next().expect("not empty");
    let mut i = after;
    while i < text.len() {
        let rest = &text[i..];
        let c = rest.chars().next().expect("i is on a char");
        if c == '\\' {
            i += 1 + rest[1..].chars().next().map_or(0, char::len_utf8);
            continue;
        }
        if c == '`' {
            let run = rest.chars().take_while(|c| *c == '`').count();
            i += closing_backticks(&rest[run..], run).map_or(run, |close| run + close + run);
            continue;
        }
        if let Some(beyond) = rest.strip_prefix(delimiter) {
            let run = rest.chars().take_while(|c| *c == mark).count();
            let before = text[..i].chars().next_back();
            let next = beyond.chars().next();
            let closes = i > after
                && before.is_some_and(|b| !b.is_whitespace())
                && !(underscore && next.is_some_and(char::is_alphanumeric));
            // A lone `*` does not close on part of a `**`.
            if closes && (!single || run == 1 || run >= 3) {
                return Some(i);
            }
            i += run;
            continue;
        }
        i += c.len_utf8();
    }
    None
}

/// Every link address in some blocks, for checking where they go.
pub fn links(blocks: &[Block]) -> Vec<String> {
    let mut found = Vec::new();
    let mut from = |text: &RichText| found.extend(text.spans.iter().filter_map(|s| s.link.clone()));
    walk(blocks, &mut from);
    for block in blocks {
        if let Block::Image { url, .. } = block {
            found.push(url.clone());
        }
    }
    found
}

/// Calls `visit` with every piece of rich text in the blocks, in order.
pub fn walk(blocks: &[Block], visit: &mut dyn FnMut(&RichText)) {
    for block in blocks {
        match block {
            Block::Heading { text, .. } | Block::Paragraph(text) => visit(text),
            Block::List { items, .. } => items.iter().for_each(|item| walk(&item.blocks, visit)),
            Block::Quote { blocks, .. } => walk(blocks, visit),
            Block::Table { head, rows, .. } => head.iter().chain(rows.iter().flatten()).for_each(&mut *visit),
            Block::Code { .. } | Block::Image { .. } | Block::Rule => {}
        }
    }
}

/// A heading's place: for "On this page" and jumping to `#anchor`.
pub fn headings(blocks: &[Block]) -> Vec<(u8, String, String)> {
    blocks
        .iter()
        .filter_map(|b| match b {
            Block::Heading { level, text, anchor } => Some((*level, text.text.clone(), anchor.clone())),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use super::*;

    fn range_of(text: &RichText, words: &str) -> Option<Range<usize>> {
        text.text.find(words).map(|at| at..at + words.len())
    }

    fn marks_of(text: &RichText, words: &str) -> Marks {
        let range = range_of(text, words).unwrap_or_else(|| panic!("{words:?} not in {:?}", text.text));
        text.spans.iter().find(|s| s.range == range).map(|s| s.marks).unwrap_or_default()
    }

    fn link_of(text: &RichText, words: &str) -> Option<String> {
        let range = range_of(text, words)?;
        text.spans.iter().find(|s| s.range == range).and_then(|s| s.link.clone())
    }

    #[test]
    fn inline_marks_and_links() {
        let text = inline("Write **bold**, *italics*, ~~struck out~~, ==highlighted== and `inline code`.");
        assert_eq!(text.text, "Write bold, italics, struck out, highlighted and inline code.");
        assert!(marks_of(&text, "bold").strong);
        assert!(marks_of(&text, "italics").emphasis);
        assert!(marks_of(&text, "struck out").strike);
        assert!(marks_of(&text, "highlighted").mark);
        assert!(marks_of(&text, "inline code").code);

        let text = inline("See [[Recipe-Design]], [[Icing-Design|the Icing design]] and [jump](#checklists).");
        assert_eq!(text.text, "See Recipe-Design, the Icing design and jump.");
        assert_eq!(link_of(&text, "Recipe-Design").as_deref(), Some("docs:Recipe-Design"));
        assert_eq!(link_of(&text, "the Icing design").as_deref(), Some("docs:Icing-Design"));
        assert_eq!(link_of(&text, "jump").as_deref(), Some("#checklists"));

        let text = inline("Go to https://docs.example.com. Or <https://voxear.dev>, and **[bold link](Docs/A.md)**.");
        assert_eq!(link_of(&text, "https://docs.example.com").as_deref(), Some("https://docs.example.com"));
        assert_eq!(link_of(&text, "https://voxear.dev").as_deref(), Some("https://voxear.dev"));
        let bold = text.spans.iter().find(|s| s.link.as_deref() == Some("Docs/A.md")).unwrap();
        assert!(bold.marks.strong, "marks reach into links");

        let text = inline(r"snake_case_words, a \*literal\* star, `code with **stars**` and ***both***");
        assert!(text.text.starts_with("snake_case_words, a *literal* star, code with **stars** and both"));
        assert_eq!(text.spans.iter().filter(|s| s.marks.emphasis && !s.marks.strong).count(), 0);
        let both = marks_of(&text, "both");
        assert!(both.strong && both.emphasis);
    }

    #[test]
    fn blocks_of_every_kind() {
        let markdown = "# Title\n\nA paragraph\nover two lines.\n\n## Lists\n\n- one\n- two\n  - nested\n\n1. first\n2. second\n\n- [x] Write the page\n- [ ] Add a picture\n\n> [!TIP]\n> A better way to do it.\n\n> A plain quote.\n\n| Attribute | Goes on |\n| --- | :---: |\n| `[Documentation]` | classes |\n\n```csharp\nvar x = 1;\n```\n\n---\n\n![A cat](images/cat.png)\n";
        let blocks = parse(markdown);
        let kinds: Vec<&str> = blocks
            .iter()
            .map(|b| match b {
                Block::Heading { .. } => "heading",
                Block::Paragraph(_) => "paragraph",
                Block::List { .. } => "list",
                Block::Quote { .. } => "quote",
                Block::Code { .. } => "code",
                Block::Table { .. } => "table",
                Block::Image { .. } => "image",
                Block::Rule => "rule",
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "heading",
                "paragraph",
                "heading",
                "list",
                "list",
                "list",
                "quote",
                "quote",
                "table",
                "code",
                "rule",
                "image"
            ]
        );
        let Block::Paragraph(text) = &blocks[1] else { unreachable!() };
        assert_eq!(text.text, "A paragraph over two lines.");
        let Block::Heading { anchor, .. } = &blocks[2] else { unreachable!() };
        assert_eq!(anchor, "lists");
        let Block::List { items, .. } = &blocks[3] else { unreachable!() };
        assert_eq!(items.len(), 2);
        assert!(matches!(items[1].blocks.last(), Some(Block::List { .. })), "the nested list is in the second item");
        let Block::List { start, .. } = &blocks[4] else { unreachable!() };
        assert_eq!(*start, Some(1));
        let Block::List { items, .. } = &blocks[5] else { unreachable!() };
        let tasks: Vec<Task> = items.iter().filter_map(|i| i.task).collect();
        assert_eq!(tasks.iter().map(|t| t.done).collect::<Vec<_>>(), [true, false]);
        assert_eq!(&markdown[tasks[0].at..tasks[0].at + 1], "x", "a task knows where its mark is");
        assert_eq!(&markdown[tasks[1].at..tasks[1].at + 1], " ");
        let Block::Quote { callout, blocks: inside } = &blocks[6] else { unreachable!() };
        assert_eq!(*callout, Some(Callout::Tip));
        assert_eq!(inside.len(), 1);
        let Block::Table { align, rows, .. } = &blocks[8] else { unreachable!() };
        assert_eq!(*align, [Align::None, Align::Center]);
        assert!(rows[0][0].spans[0].marks.code);
        let Block::Code { language, code } = &blocks[9] else { unreachable!() };
        assert_eq!((language.as_str(), code.as_str()), ("csharp", "var x = 1;"));
        let Block::Image { alt, url } = &blocks[11] else { unreachable!() };
        assert_eq!((alt.as_str(), url.as_str()), ("A cat", "images/cat.png"));
    }

    #[test]
    fn headings_get_unique_anchors() {
        let blocks = parse("## Set up\n\n## Set up\n\nTitle\n=====\n");
        let anchors: Vec<String> = headings(&blocks).into_iter().map(|(_, _, a)| a).collect();
        assert_eq!(anchors, ["set-up", "set-up-1", "title"]);
    }
}
