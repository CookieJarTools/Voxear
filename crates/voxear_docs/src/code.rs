//! The Code Reference: a page for every public type of the game's scripts
//! and of Voxear.Core, made from the code and its `///` comments. The
//! engine's script tool writes it as `Code.json` each time the scripts are
//! described, and VoxDocs shows it after the project's own pages, a folder
//! for each namespace, the game's first, and in it the folders the types are
//! written in. Its pages are read, not written: they change when the code
//! does.
//!
//! Any page links to a type as `[[code:Voxear.Prefab]]`, or
//! `[[code:Voxear.Prefab|Prefab]]` to word the link itself.

use std::path::Path;

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

/// What a link to a type starts with, before the type's full name.
pub const CODE_SCHEME: &str = "code:";
/// The newest `Code.json` this reads.
const FORMAT: u32 = 1;
/// The assembly the engine's types are in, whose namespaces come after the game's.
const ENGINE: &str = "Voxear.Core";
/// Each kind of member, and its section's heading, in the order a page shows them.
const SECTIONS: [(&str, &str); 6] = [
    ("constructor", "Constructors"),
    ("value", "Values"),
    ("field", "Fields"),
    ("property", "Properties"),
    ("method", "Methods"),
    ("event", "Events"),
];
/// What a member with no `///` comment says instead.
const UNDESCRIBED: &str = "*No description yet.*";

/// Every type the Code Reference has a page for, by full name.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct CodeReference {
    format: u32,
    types: Vec<CodeType>,
}

/// A type and its members, as its page shows them. The words are Markdown.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct CodeType {
    /// Its full name, as links give it: `Voxear.Prefab`, or ``Voxear.Ref`1``
    /// for a generic type.
    pub key: String,
    /// Empty for the global namespace.
    pub namespace: String,
    /// Its name as code writes it: `Ref<T>`.
    pub name: String,
    /// class, struct, interface, enum or delegate.
    pub kind: String,
    pub declaration: String,
    pub assembly: String,
    /// The folder its source is in, from the folder all its assembly's
    /// sources share, as `Inspector/Layout`; empty at the top.
    pub folder: String,
    pub summary: String,
    pub remarks: String,
    /// The VoxDocs page its `[Documentation]` names, or nothing.
    pub documentation: String,
    pub members: Vec<Member>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Member {
    /// constructor, method, property, field, value (of an enum) or event.
    pub kind: String,
    /// Its heading: `Load(string)`.
    pub name: String,
    pub declaration: String,
    pub summary: String,
    pub returns: String,
    /// The VoxDocs page its `[Documentation]` or `[Excerpt]` names, or nothing.
    pub documentation: String,
    pub parameters: Vec<Parameter>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Parameter {
    pub name: String,
    pub text: String,
}

impl CodeReference {
    pub fn read(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("reading {}", path.display()))
    }

    pub fn parse(text: &str) -> Result<Self> {
        let reference: Self = serde_json::from_str(text)?;
        ensure!(reference.format <= FORMAT, "it is in format {}, which this Voxear cannot read", reference.format);
        Ok(reference)
    }

    pub fn types(&self) -> &[CodeType] {
        &self.types
    }

    /// The namespaces with types in them: the game's first, then the
    /// engine's, each in order of name. The global namespace is `""`.
    pub fn namespaces(&self) -> Vec<&str> {
        let mut found: Vec<(bool, &str)> =
            self.types.iter().map(|t| (t.assembly == ENGINE, t.namespace.as_str())).collect();
        found.sort_unstable();
        let mut namespaces: Vec<&str> = Vec::new();
        for (_, namespace) in found {
            if !namespaces.contains(&namespace) {
                namespaces.push(namespace);
            }
        }
        namespaces
    }
}

impl CodeType {
    /// Its title: its name, after any types it is nested in, `Outer.Inner`.
    pub fn title(&self) -> String {
        let local = self.key.strip_prefix(&format!("{}.", self.namespace)).unwrap_or(&self.key);
        match local.rsplit_once('.') {
            Some((outer, _)) => format!("{}.{}", without_arity(outer), self.name),
            None => self.name.clone(),
        }
    }

    /// The folders its page is in under its namespace's: those its source is
    /// in, less any the namespace already names, so `Voxear.UI` types in
    /// `UI/Controls` go in `Controls`.
    pub fn folders(&self) -> Vec<&str> {
        let mut folders: Vec<&str> = self.folder.split('/').filter(|f| !f.is_empty()).collect();
        let named: Vec<&str> = self.namespace.split('.').collect();
        let skip = (1..=named.len().min(folders.len()))
            .rev()
            .find(|&count| folders[..count] == named[named.len() - count..])
            .unwrap_or(0);
        folders.drain(..skip);
        folders
    }

    /// Its page: how it is declared, what its comment says and where it is,
    /// then its members, a section for each kind. `title_of` names the pages
    /// `[Documentation]` points at, when they are there.
    pub fn page(&self, title_of: &dyn Fn(&str) -> Option<String>) -> String {
        let mut page = format!("```csharp\n{}\n```\n\n{}\n\n", self.declaration, described(&self.summary));
        if !self.remarks.is_empty() {
            page += &format!("{}\n\n", self.remarks);
        }
        page += &documented(&self.documentation, title_of);
        page += &match self.namespace.as_str() {
            "" => format!("In the global namespace, in {}.\n", self.assembly),
            namespace => format!("In the `{namespace}` namespace, in {}.\n", self.assembly),
        };
        for (kind, heading) in SECTIONS {
            let members: Vec<&Member> = self.members.iter().filter(|m| m.kind == kind).collect();
            if members.is_empty() {
                continue;
            }
            page += &format!("\n## {heading}\n\n");
            for member in members {
                page += &if kind == "value" { member.value() } else { member.section(title_of) };
            }
        }
        page
    }
}

impl Member {
    /// A method, property or the like: its heading, how it is declared, and
    /// what its comment says of it, its parameters and what it returns.
    fn section(&self, title_of: &dyn Fn(&str) -> Option<String>) -> String {
        let mut section = format!(
            "### {}\n\n```csharp\n{}\n```\n\n{}\n\n{}",
            escaped(&self.name),
            self.declaration,
            described(&self.summary),
            documented(&self.documentation, title_of)
        );
        let parameters: Vec<&Parameter> = self.parameters.iter().filter(|p| !p.text.is_empty()).collect();
        if !parameters.is_empty() {
            section += "**Parameters**\n\n";
            for parameter in parameters {
                section += &format!("- `{}`: {}\n", parameter.name, parameter.text);
            }
            section += "\n";
        }
        if !self.returns.is_empty() {
            section += &format!("**Returns** {}\n\n", self.returns);
        }
        section
    }

    /// An enum's value, a line in a list, since most need no more.
    fn value(&self) -> String {
        match self.summary.as_str() {
            "" => format!("- `{}`\n", self.declaration),
            summary => format!("- `{}`: {summary}\n", self.declaration),
        }
    }
}

/// The line saying which VoxDocs page explains it, linked by the page's
/// title when the page is there, or nothing.
fn documented(reference: &str, title_of: &dyn Fn(&str) -> Option<String>) -> String {
    match title_of(reference) {
        _ if reference.is_empty() => String::new(),
        Some(title) => format!("Documented in [[{reference}|{title}]].\n\n"),
        None => format!("Documented in [[{reference}]].\n\n"),
    }
}

fn described(summary: &str) -> &str {
    if summary.is_empty() { UNDESCRIBED } else { summary }
}

/// A name with what Markdown would read as marks escaped, as a heading
/// shows it: `Find<T>(T[])`.
fn escaped(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if matches!(c, '\\' | '`' | '*' | '_' | '[' | ']' | '|' | '<' | '>') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// A full name without its generic arities: ``Outer`1.Middle`` is `Outer.Middle`.
fn without_arity(name: &str) -> String {
    name.split('.').map(|part| part.split('`').next().unwrap_or(part)).collect::<Vec<_>>().join(".")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "format": 1,
        "types": [
            { "key": "Voxear.Prefab", "namespace": "Voxear", "name": "Prefab", "kind": "struct",
              "declaration": "public struct Prefab", "assembly": "Voxear.Core",
              "summary": "A loaded prefab.", "remarks": "",
              "members": [
                { "kind": "method", "name": "Load(string)", "declaration": "public static Prefab Load(string path)",
                  "summary": "Loads a prefab.", "returns": "The prefab.",
                  "parameters": [{ "name": "path", "text": "Where it is." }] },
                { "kind": "field", "name": "None", "declaration": "public static readonly Prefab None",
                  "summary": "", "returns": "", "parameters": [] }
              ] },
            { "key": "Hello.Spinner", "namespace": "Hello", "name": "Spinner", "kind": "class",
              "declaration": "public sealed class Spinner : Behaviour", "assembly": "Hello.Scripts",
              "summary": "Turns its Obj. See [[code:Voxear.Prefab|Prefab]].", "remarks": "", "members": [] },
            { "key": "Hello.Grid`1.Cell", "namespace": "Hello", "name": "Cell", "kind": "enum",
              "declaration": "public enum Cell", "assembly": "Hello.Scripts", "summary": "", "remarks": "",
              "members": [{ "kind": "value", "name": "Empty", "declaration": "Empty = 0", "summary": "Nothing there." }] }
        ]
    }"#;

    #[test]
    fn the_games_namespaces_come_before_the_engines() {
        let code = CodeReference::parse(SAMPLE).unwrap();
        assert_eq!(code.namespaces(), ["Hello", "Voxear"]);
        assert_eq!(code.types()[2].title(), "Grid.Cell", "nested, without its arity");
    }

    #[test]
    fn a_page_shows_the_declaration_comments_and_members_by_kind() {
        let code = CodeReference::parse(SAMPLE).unwrap();
        let page = code.types()[0].page(&|_| None);
        assert!(page.starts_with("```csharp\npublic struct Prefab\n```\n\nA loaded prefab.\n\n"), "{page}");
        assert!(page.contains("In the `Voxear` namespace, in Voxear.Core."), "{page}");
        let fields = page.find("## Fields").unwrap();
        let methods = page.find("## Methods").unwrap();
        assert!(fields < methods, "fields come before methods: {page}");
        let none = "### None\n\n```csharp\npublic static readonly Prefab None\n```\n\n*No description yet.*";
        assert!(page.contains(none), "{page}");
        assert!(page.contains("- `path`: Where it is.\n"), "{page}");
        assert!(page.contains("**Returns** The prefab."), "{page}");
        let values = code.types()[2].page(&|_| None);
        assert!(values.contains("## Values\n\n- `Empty = 0`: Nothing there.\n"), "{values}");
    }

    #[test]
    fn folders_the_namespace_names_are_left_out() {
        let kind = |namespace: &str, folder: &str| CodeType {
            namespace: namespace.into(),
            folder: folder.into(),
            ..CodeType::default()
        };
        assert_eq!(kind("Voxear", "Inspector/Layout").folders(), ["Inspector", "Layout"]);
        assert_eq!(kind("Voxear.UI", "UI/Controls").folders(), ["Controls"]);
        assert_eq!(kind("Hello.Enemies.Bosses", "Enemies/Bosses").folders(), Vec::<&str>::new());
        assert!(kind("Hello", "").folders().is_empty());
    }

    #[test]
    fn a_newer_format_is_not_read() {
        let error = CodeReference::parse(r#"{ "format": 9, "types": [] }"#).unwrap_err();
        assert!(error.to_string().contains("format 9"), "{error}");
    }

    #[test]
    fn marks_in_a_heading_are_escaped() {
        assert_eq!(escaped("Find<T>(T[])"), r"Find\<T\>(T\[\])");
    }
}
