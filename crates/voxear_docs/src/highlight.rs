//! Colours for code, as a code editor gives them: keywords, types, calls,
//! strings, numbers, comments, attributes, tags and keys, read by a small
//! reader for each language VoxDocs knows. C# and Rust share one, as do the
//! editor's own `.vui` markup and XML, and `.vss` style sheets and CSS;
//! TOML, JSON and shell have their own. Any other language is drawn plain.
//! The readers only colour: code they do not follow is left as it is.

use std::ops::Range;

use voxear_ui::{Color, RichText};

/// What a piece of code is, for its colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Keyword,
    /// A type's name, as C# and Rust write them, starting with a capital.
    Type,
    /// A name called, or a shell command.
    Call,
    String,
    Number,
    Comment,
    /// C#'s `[Attribute]`, Rust's `#[attribute]`, and `#if` and the like.
    Attribute,
    /// A markup tag, or a TOML table's name.
    Tag,
    /// A TOML key, a markup attribute, a style sheet's property, or a flag.
    Key,
}

/// Words C# keeps for itself, its built-in types among them.
#[rustfmt::skip]
const CSHARP: &[&str] = &[
    "abstract", "as", "async", "await", "base", "bool", "break", "byte", "case", "catch", "char", "class", "const",
    "continue", "decimal", "default", "delegate", "do", "double", "else", "enum", "event", "explicit", "extern",
    "false", "finally", "fixed", "float", "for", "foreach", "get", "goto", "if", "implicit", "in", "init", "int",
    "interface", "internal", "is", "lock", "long", "namespace", "new", "null", "object", "operator", "out",
    "override", "params", "private", "protected", "public", "readonly", "record", "ref", "return", "sbyte",
    "sealed", "set", "short", "sizeof", "static", "string", "struct", "switch", "this", "throw", "true", "try",
    "typeof", "uint", "ulong", "unsafe", "ushort", "using", "var", "virtual", "void", "volatile", "when", "where",
    "while", "with", "yield",
];

/// Words Rust keeps for itself, its built-in types among them.
#[rustfmt::skip]
const RUST: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false", "fn",
    "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self",
    "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where", "while", "bool", "char",
    "f32", "f64", "i8", "i16", "i32", "i64", "isize", "str", "u8", "u16", "u32", "u64", "usize",
];

/// Values TOML and JSON write as words.
const LITERALS: &[&str] = &["true", "false", "null", "inf", "nan"];

/// How many columns a tab in code reaches to, as code editors show them.
const TAB: usize = 4;

/// The pieces of `code` to colour, in order, for code in `language` as a
/// fence names it (`csharp`, `rust`, `toml`…). Unknown languages get none.
pub fn highlight(language: &str, code: &str) -> Vec<(Range<usize>, Kind)> {
    match language.to_ascii_lowercase().as_str() {
        "csharp" | "cs" | "c#" => c_like(code, CSHARP, true),
        "rust" | "rs" => c_like(code, RUST, false),
        "toml" => toml(code),
        "json" => json(code),
        "xml" | "vui" | "html" => markup(code),
        "css" | "vss" => sheet(code),
        "bash" | "sh" | "shell" | "console" => shell(code),
        _ => Vec::new(),
    }
}

/// `code` as rich text in a code editor's colours, for a dark or a light
/// ground.
pub fn colored(language: &str, code: &str, dark: bool) -> RichText {
    let mut text = RichText::new();
    let mut at = 0;
    for (range, kind) in &highlight(language, code) {
        if range.start < at || range.end > code.len() {
            continue;
        }
        text.push(&code[at..range.start], Default::default());
        text.push_colored(&code[range.clone()], color(*kind, dark));
        at = range.end;
    }
    text.push(&code[at..], Default::default());
    text
}

/// Each kind's colour, picked to read on a code block's ground in a dark
/// theme or a light one.
pub fn color(kind: Kind, dark: bool) -> Color {
    let hex = match (kind, dark) {
        (Kind::Keyword, true) => 0xc792ea,
        (Kind::Keyword, false) => 0x8a3fbf,
        (Kind::Type, true) => 0x7cc4fa,
        (Kind::Type, false) => 0x1f6fb2,
        (Kind::Call, true) => 0xe8c37e,
        (Kind::Call, false) => 0x9a6200,
        (Kind::String, true) => 0xa5d6a7,
        (Kind::String, false) => 0x2e7d32,
        (Kind::Number, true) => 0xf2a97c,
        (Kind::Number, false) => 0xb4501c,
        (Kind::Comment, true) => 0x7d8597,
        (Kind::Comment, false) => 0x6b7280,
        (Kind::Attribute, true) => 0xd7ba7d,
        (Kind::Attribute, false) => 0x8a6d1c,
        (Kind::Tag, true) => 0x7cc4fa,
        (Kind::Tag, false) => 0x1f6fb2,
        (Kind::Key, true) => 0x9cdcfe,
        (Kind::Key, false) => 0x0b5f8a,
    };
    Color::hex(hex)
}

/// Code as it shows: each tab out to the next of every [`TAB`] columns,
/// since the fonts draw no tab of their own.
pub fn expand_tabs(code: &str) -> String {
    let mut out = String::with_capacity(code.len());
    for line in code.split_inclusive('\n') {
        let mut column = 0;
        for c in line.chars() {
            if c == '\t' {
                let spaces = TAB - column % TAB;
                out.extend(std::iter::repeat_n(' ', spaces));
                column += spaces;
            } else {
                out.push(c);
                column += 1;
            }
        }
    }
    out
}

/// A reader's place in the code.
struct Reader<'a> {
    code: &'a str,
    bytes: &'a [u8],
    at: usize,
    out: Vec<(Range<usize>, Kind)>,
}

impl<'a> Reader<'a> {
    fn new(code: &'a str) -> Self {
        Self { code, bytes: code.as_bytes(), at: 0, out: Vec::new() }
    }

    fn peek(&self, ahead: usize) -> u8 {
        self.bytes.get(self.at + ahead).copied().unwrap_or(0)
    }

    fn starts(&self, text: &str) -> bool {
        self.code[self.at..].starts_with(text)
    }

    fn mark(&mut self, start: usize, kind: Kind) {
        if self.at > start {
            self.out.push((start..self.at, kind));
        }
    }

    /// Moves to the end of the line, not past it.
    fn skip_line(&mut self) {
        while self.at < self.bytes.len() && self.bytes[self.at] != b'\n' {
            self.at += 1;
        }
    }

    /// Moves past `end`, or to the end of the code.
    fn past(&mut self, end: &str) {
        match self.code[self.at..].find(end) {
            Some(found) => self.at += found + end.len(),
            None => self.at = self.bytes.len(),
        }
    }

    /// A string starting at a quote: past its closing quote, minding
    /// backslashes unless `verbatim`, where a doubled quote stands for one.
    fn string(&mut self, quote: u8, verbatim: bool) {
        self.at += 1;
        while self.at < self.bytes.len() {
            let byte = self.bytes[self.at];
            if !verbatim && byte == b'\\' {
                self.at += 2;
                continue;
            }
            if byte == quote {
                if verbatim && self.peek(1) == quote {
                    self.at += 2;
                    continue;
                }
                self.at += 1;
                return;
            }
            if byte == b'\n' && !verbatim {
                return;
            }
            self.at += 1;
        }
        self.at = self.at.min(self.bytes.len());
    }

    fn word(&mut self) -> &'a str {
        let start = self.at;
        while self.at < self.bytes.len() && (self.bytes[self.at].is_ascii_alphanumeric() || self.bytes[self.at] == b'_')
        {
            self.at += 1;
        }
        &self.code[start..self.at]
    }

    fn number(&mut self) {
        while self.at < self.bytes.len() && (self.bytes[self.at].is_ascii_alphanumeric() || self.bytes[self.at] == b'.')
        {
            if self.bytes[self.at] == b'.' && !self.peek(1).is_ascii_digit() {
                break;
            }
            self.at += 1;
        }
    }

    /// The next byte that is not a space or a tab, from `from`.
    fn next_solid(&self, from: usize) -> u8 {
        self.bytes[from.min(self.bytes.len())..].iter().copied().find(|b| *b != b' ' && *b != b'\t').unwrap_or(0)
    }

    /// Whether only spaces come before `at` on its line.
    fn line_starts_at(&self, at: usize) -> bool {
        self.bytes[..at].iter().rev().take_while(|b| **b != b'\n').all(|b| *b == b' ' || *b == b'\t')
    }

    /// One byte, or a whole character, along.
    fn step(&mut self) {
        self.at += self.code[self.at..].chars().next().map_or(1, char::len_utf8);
    }
}

/// C# and Rust: comments, strings and characters, numbers, keywords, types
/// by their capital, calls before a bracket, and attributes.
fn c_like(code: &str, keywords: &[&str], csharp: bool) -> Vec<(Range<usize>, Kind)> {
    let mut read = Reader::new(code);
    while read.at < read.bytes.len() {
        let start = read.at;
        let byte = read.peek(0);
        if read.starts("//") {
            read.skip_line();
            read.mark(start, Kind::Comment);
        } else if read.starts("/*") {
            read.past("*/");
            read.mark(start, Kind::Comment);
        } else if csharp && byte == b'#' && read.line_starts_at(start) {
            read.skip_line();
            read.mark(start, Kind::Attribute);
        } else if attribute_starts(&read, csharp) {
            read.past("]");
            read.mark(start, Kind::Attribute);
        } else if csharp && read.starts("\"\"\"") {
            read.at += 3;
            read.past("\"\"\"");
            read.mark(start, Kind::String);
        } else if csharp && (read.starts("@\"") || read.starts("$@\"") || read.starts("@$\"")) {
            read.at += read.code[read.at..].find('"').unwrap_or(0);
            read.string(b'"', true);
            read.mark(start, Kind::String);
        } else if csharp && read.starts("$\"") {
            read.at += 1;
            read.string(b'"', false);
            read.mark(start, Kind::String);
        } else if byte == b'"' {
            read.string(b'"', false);
            read.mark(start, Kind::String);
        } else if byte == b'\'' && (csharp || read.peek(2) == b'\'' || read.peek(1) == b'\\') {
            read.string(b'\'', false);
            read.mark(start, Kind::String);
        } else if byte.is_ascii_digit() {
            read.number();
            read.mark(start, Kind::Number);
        } else if byte.is_ascii_alphabetic() || byte == b'_' {
            let word = read.word();
            let next = read.next_solid(read.at);
            let kind = if keywords.contains(&word) {
                Some(Kind::Keyword)
            } else if next == b'(' || (!csharp && next == b'!') {
                Some(Kind::Call)
            } else if word.starts_with(|c: char| c.is_ascii_uppercase()) {
                Some(Kind::Type)
            } else {
                None
            };
            if let Some(kind) = kind {
                read.mark(start, kind);
            }
        } else {
            read.step();
        }
    }
    read.out
}

/// Whether an attribute starts here: C#'s `[Name` at the start of a line,
/// or Rust's `#[` and `#![`.
fn attribute_starts(read: &Reader<'_>, csharp: bool) -> bool {
    if csharp {
        read.peek(0) == b'[' && read.line_starts_at(read.at) && read.peek(1).is_ascii_alphabetic()
    } else {
        read.starts("#[") || read.starts("#![")
    }
}

/// TOML: comments, `[tables]`, keys before `=`, strings, numbers and
/// `true` and `false`.
fn toml(code: &str) -> Vec<(Range<usize>, Kind)> {
    let mut read = Reader::new(code);
    while read.at < read.bytes.len() {
        let start = read.at;
        let byte = read.peek(0);
        if byte == b'#' {
            read.skip_line();
            read.mark(start, Kind::Comment);
        } else if byte == b'[' && read.line_starts_at(start) {
            read.past("]");
            while read.peek(0) == b']' {
                read.at += 1;
            }
            read.mark(start, Kind::Tag);
        } else if byte == b'"' || byte == b'\'' {
            read.string(byte, byte == b'\'');
            let is_key = read.next_solid(read.at) == b'=';
            read.mark(start, if is_key { Kind::Key } else { Kind::String });
        } else if byte.is_ascii_digit() || (byte == b'-' && read.peek(1).is_ascii_digit()) {
            read.at += 1;
            read.number();
            read.mark(start, Kind::Number);
        } else if byte.is_ascii_alphabetic() || byte == b'_' {
            let word = read.word();
            while read.peek(0) == b'.' || read.peek(0) == b'-' {
                read.at += 1;
                read.word();
            }
            if read.next_solid(read.at) == b'=' {
                read.mark(start, Kind::Key);
            } else if LITERALS.contains(&word) {
                read.mark(start, Kind::Keyword);
            }
        } else {
            read.step();
        }
    }
    read.out
}

/// JSON: keys, strings, numbers, and `true`, `false` and `null`.
fn json(code: &str) -> Vec<(Range<usize>, Kind)> {
    let mut read = Reader::new(code);
    while read.at < read.bytes.len() {
        let start = read.at;
        let byte = read.peek(0);
        if byte == b'"' {
            read.string(b'"', false);
            let is_key = read.next_solid(read.at) == b':';
            read.mark(start, if is_key { Kind::Key } else { Kind::String });
        } else if byte.is_ascii_digit() || (byte == b'-' && read.peek(1).is_ascii_digit()) {
            read.at += 1;
            read.number();
            read.mark(start, Kind::Number);
        } else if byte.is_ascii_alphabetic() {
            let word = read.word();
            if LITERALS.contains(&word) {
                read.mark(start, Kind::Keyword);
            }
        } else {
            read.step();
        }
    }
    read.out
}

/// Markup, as `.vui` and XML: comments, tags by name, attributes and their
/// values; text between tags is left plain.
fn markup(code: &str) -> Vec<(Range<usize>, Kind)> {
    let mut read = Reader::new(code);
    while read.at < read.bytes.len() {
        let start = read.at;
        if read.starts("<!--") {
            read.past("-->");
            read.mark(start, Kind::Comment);
        } else if read.peek(0) == b'<' {
            read.at += 1;
            if read.peek(0) == b'/' {
                read.at += 1;
            }
            read.word();
            read.mark(start, Kind::Tag);
            tag_inside(&mut read);
        } else {
            read.step();
        }
    }
    read.out
}

/// A tag's attributes and their values, and its closing bracket.
fn tag_inside(read: &mut Reader<'_>) {
    while read.at < read.bytes.len() {
        let start = read.at;
        let byte = read.peek(0);
        if byte == b'>' || read.starts("/>") {
            read.at += if byte == b'>' { 1 } else { 2 };
            read.mark(start, Kind::Tag);
            return;
        } else if byte == b'"' || byte == b'\'' {
            read.string(byte, true);
            read.mark(start, Kind::String);
        } else if byte.is_ascii_alphabetic() {
            while read.at < read.bytes.len()
                && (read.bytes[read.at].is_ascii_alphanumeric() || matches!(read.bytes[read.at], b'-' | b'_' | b':'))
            {
                read.at += 1;
            }
            read.mark(start, Kind::Key);
        } else {
            read.step();
        }
    }
}

/// Style sheets, as `.vss` and CSS: comments, properties inside braces,
/// numbers with their units and colours, strings, and `@` rules.
fn sheet(code: &str) -> Vec<(Range<usize>, Kind)> {
    let mut read = Reader::new(code);
    let mut depth = 0;
    while read.at < read.bytes.len() {
        let start = read.at;
        let byte = read.peek(0);
        if read.starts("/*") {
            read.past("*/");
            read.mark(start, Kind::Comment);
        } else if byte == b'{' || byte == b'}' {
            depth = if byte == b'{' { depth + 1 } else { (depth - 1).max(0) };
            read.at += 1;
        } else if byte == b'"' || byte == b'\'' {
            read.string(byte, false);
            read.mark(start, Kind::String);
        } else if byte == b'@' {
            read.at += 1;
            read.word();
            read.mark(start, Kind::Keyword);
        } else if byte == b'#' && depth > 0 {
            read.at += 1;
            read.word();
            read.mark(start, Kind::Number);
        } else if byte.is_ascii_digit() || (byte == b'.' && read.peek(1).is_ascii_digit()) {
            read.number();
            while read.peek(0) == b'%' {
                read.at += 1;
            }
            read.mark(start, Kind::Number);
        } else if depth > 0 && (byte.is_ascii_alphabetic() || byte == b'-') {
            while read.at < read.bytes.len()
                && (read.bytes[read.at].is_ascii_alphanumeric() || read.bytes[read.at] == b'-')
            {
                read.at += 1;
            }
            match read.next_solid(read.at) {
                b':' => read.mark(start, Kind::Key),
                b'(' => read.mark(start, Kind::Call),
                _ => {}
            }
        } else if depth == 0 && (byte == b'.' || byte == b'#' || byte.is_ascii_alphabetic()) {
            read.at += 1;
            while read.at < read.bytes.len()
                && (read.bytes[read.at].is_ascii_alphanumeric() || matches!(read.bytes[read.at], b'-' | b'_'))
            {
                read.at += 1;
            }
            read.mark(start, Kind::Tag);
        } else {
            read.step();
        }
    }
    read.out
}

/// Shell commands: comments, strings, the command each line starts with,
/// and its flags.
fn shell(code: &str) -> Vec<(Range<usize>, Kind)> {
    let mut read = Reader::new(code);
    while read.at < read.bytes.len() {
        let start = read.at;
        let byte = read.peek(0);
        if byte == b'#' && (start == 0 || matches!(read.bytes[start - 1], b' ' | b'\t' | b'\n')) {
            read.skip_line();
            read.mark(start, Kind::Comment);
        } else if byte == b'"' || byte == b'\'' {
            read.string(byte, byte == b'\'');
            read.mark(start, Kind::String);
        } else if byte == b'-' && (start == 0 || read.bytes[start - 1] == b' ') {
            while read.at < read.bytes.len() && !matches!(read.bytes[read.at], b' ' | b'\n' | b'=') {
                read.at += 1;
            }
            read.mark(start, Kind::Key);
        } else if byte.is_ascii_alphabetic() && read.line_starts_at(start) {
            while read.at < read.bytes.len() && !matches!(read.bytes[read.at], b' ' | b'\n') {
                read.at += 1;
            }
            read.mark(start, Kind::Call);
        } else {
            read.step();
        }
    }
    read.out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds<'a>(code: &'a str, pieces: &[(Range<usize>, Kind)]) -> Vec<(&'a str, Kind)> {
        pieces.iter().map(|(range, kind)| (&code[range.clone()], *kind)).collect()
    }

    #[test]
    fn csharp_has_its_keywords_types_calls_strings_and_attributes() {
        let code = "[SerializeField] private float speed = 2.5f; // how fast\nvar turn = Quaternion.AngleAxis(speed, \"up\");\n#if VOXEAR_EDITOR";
        let found = kinds(code, &highlight("csharp", code));
        assert_eq!(
            found,
            [
                ("[SerializeField]", Kind::Attribute),
                ("private", Kind::Keyword),
                ("float", Kind::Keyword),
                ("2.5f", Kind::Number),
                ("// how fast", Kind::Comment),
                ("var", Kind::Keyword),
                ("Quaternion", Kind::Type),
                ("AngleAxis", Kind::Call),
                ("\"up\"", Kind::String),
                ("#if VOXEAR_EDITOR", Kind::Attribute),
            ]
        );
    }

    #[test]
    fn toml_markup_and_sheets_have_their_own_parts() {
        let toml = "[obj.renderer] # drawn\nmesh = \"builtin:cube\"\nscale = 2.0\n";
        assert_eq!(
            kinds(toml, &highlight("toml", toml)),
            [
                ("[obj.renderer]", Kind::Tag),
                ("# drawn", Kind::Comment),
                ("mesh", Kind::Key),
                ("\"builtin:cube\"", Kind::String),
                ("scale", Kind::Key),
                ("2.0", Kind::Number)
            ]
        );
        let vui = "<Panel name=\"hud\">0</Panel>";
        assert_eq!(
            kinds(vui, &highlight("vui", vui)),
            [
                ("<Panel", Kind::Tag),
                ("name", Kind::Key),
                ("\"hud\"", Kind::String),
                (">", Kind::Tag),
                ("</Panel", Kind::Tag),
                (">", Kind::Tag)
            ]
        );
        let vss = ".hud { gap: 8px; color: #ffffff; }";
        assert_eq!(
            kinds(vss, &highlight("vss", vss)),
            [
                (".hud", Kind::Tag),
                ("gap", Kind::Key),
                ("8px", Kind::Number),
                ("color", Kind::Key),
                ("#ffffff", Kind::Number)
            ]
        );
    }

    #[test]
    fn tabs_in_code_show_as_spaces_to_the_next_stop() {
        assert_eq!(expand_tabs("{\n\tint x;\n\t\treturn;\n}"), "{\n    int x;\n        return;\n}");
        assert_eq!(expand_tabs("ab\tc"), "ab  c", "out to the next stop, not four more");
        assert_eq!(expand_tabs("no tabs"), "no tabs");
    }

    #[test]
    fn colored_text_keeps_every_letter_and_an_unknown_language_is_plain() {
        let code = "let x = 1; // one";
        let text = colored("rust", code, true);
        assert_eq!(text.text, code);
        assert_eq!(text.spans.len(), 3, "let, 1 and the comment");
        assert!(colored("brainfudge", code, true).spans.is_empty());
    }
}
