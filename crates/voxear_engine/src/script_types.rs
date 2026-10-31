//! What a project's behaviours are, read from their C# sources without
//! building them, for the editor: each class deriving from `Behaviour`,
//! directly or through another, with the fields a scene can set (public, or
//! private with `[SerializeField]`, and neither `static`, `const`,
//! `readonly` nor `[NonSerialized]`), their C# types, and the values they
//! start with when those are plain literals.
//!
//! It reads the code as text, which is quick and needs no build, but is a
//! good guess rather than the compiler's word: a field declared together
//! with another on one line is missed, and so are generic and nested
//! classes, which scenes cannot name anyway.

use std::path::{Path, PathBuf};

use crate::Project;

/// A behaviour class, and the fields a scene can set on it.
#[derive(Clone, Debug, PartialEq)]
pub struct BehaviourType {
    /// The full name, as scenes write it: `Hello.Spinner`.
    pub name: String,
    /// Base classes' fields first, then its own, in the order declared.
    pub fields: Vec<ScriptField>,
}

impl BehaviourType {
    /// The class name without its namespace.
    pub fn short_name(&self) -> &str {
        self.name.rsplit('.').next().unwrap_or(&self.name)
    }

    /// The field a scene's key names: keys match ignoring case and
    /// underscores, as `BehaviourFields` sets them.
    pub fn field(&self, key: &str) -> Option<&ScriptField> {
        self.fields.iter().find(|f| same_field(&f.name, key))
    }
}

/// One field a scene can set.
#[derive(Clone, Debug, PartialEq)]
pub struct ScriptField {
    pub name: String,
    /// The C# type as written, such as `float`, `Vector3` or `Spinner?`.
    pub type_name: String,
    /// What it starts as, when that is a number, string or bool literal.
    pub default: Option<toml::Value>,
}

/// Whether two field names are the same to a scene: ignoring case and
/// underscores, so `speed_limit` sets `SpeedLimit`.
pub fn same_field(a: &str, b: &str) -> bool {
    let plain = |name: &str| name.chars().filter(|&c| c != '_').flat_map(char::to_lowercase).collect::<String>();
    plain(a) == plain(b)
}

/// Every behaviour the project's scripts declare that a scene can add,
/// sorted by name. Empty for a project without scripts.
pub fn behaviour_types(project: &Project) -> Vec<BehaviourType> {
    let Some(folder) = project.scripts_project().and_then(|csproj| csproj.parent().map(Path::to_path_buf)) else {
        return Vec::new();
    };
    let mut files = Vec::new();
    sources(&folder, &mut files);
    let texts: Vec<String> = files.iter().filter_map(|file| std::fs::read_to_string(file).ok()).collect();
    scan(&texts.iter().map(String::as_str).collect::<Vec<_>>())
}

/// The `.cs` files in a folder and those inside it, leaving out build output.
fn sources(folder: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() && !matches!(name.as_str(), "bin" | "obj") && !name.starts_with('.') {
            sources(&path, files);
        } else if name.ends_with(".cs") {
            files.push(path);
        }
    }
}

/// A class as declared: its full name, the first type it derives from, and
/// its own fields.
struct Declared {
    name: String,
    namespace: String,
    base: Option<String>,
    is_abstract: bool,
    fields: Vec<ScriptField>,
}

/// Reads the behaviour types out of C# source texts.
pub fn scan(texts: &[&str]) -> Vec<BehaviourType> {
    let mut declared = Vec::new();
    for text in texts {
        declared.extend(Reader::default().read(&strip_comments(text)));
    }
    let short = |name: &str| name.rsplit('.').next().unwrap_or(name).to_string();
    // A class's base, found among the declared ones by its short name.
    let base_of = |class: &Declared| {
        let base = short(class.base.as_deref()?);
        declared
            .iter()
            .find(|d| short(&d.name) == base && d.namespace == class.namespace)
            .or_else(|| declared.iter().find(|d| short(&d.name) == base))
    };
    let mut types = Vec::new();
    for class in declared.iter().filter(|c| !c.is_abstract) {
        let mut chain = vec![class];
        let mut is_behaviour = false;
        while let Some(&at) = chain.last() {
            if at.base.as_deref().is_some_and(|b| matches!(b, "Behaviour" | "Voxear.Behaviour")) {
                is_behaviour = true;
                break;
            }
            match base_of(at).filter(|b| !chain.iter().any(|c| std::ptr::eq(*c, *b))) {
                Some(base) => chain.push(base),
                None => break,
            }
        }
        if !is_behaviour {
            continue;
        }
        let fields = chain.iter().rev().flat_map(|c| c.fields.iter().cloned()).collect();
        types.push(BehaviourType { name: class.name.clone(), fields });
    }
    types.sort_by(|a, b| a.name.cmp(&b.name));
    types
}

/// The text without `//` and `/* */` comments, keeping strings whole.
fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut string: Option<char> = None;
    while let Some(c) = chars.next() {
        if let Some(quote) = string {
            out.push(c);
            if c == '\\' {
                out.extend(chars.next());
            } else if c == quote {
                string = None;
            }
            continue;
        }
        match (c, chars.peek()) {
            ('/', Some('/')) => {
                for skipped in chars.by_ref() {
                    if skipped == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut last = ' ';
                for skipped in chars.by_ref() {
                    if last == '*' && skipped == '/' {
                        break;
                    }
                    last = skipped;
                }
                out.push(' ');
            }
            ('"' | '\'', _) => {
                string = Some(c);
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// What a pair of braces holds.
enum Scope {
    Namespace(String),
    /// A class or struct: its index in what was read, or `None` for one
    /// that is not read (nested, generic, or not a class).
    Type(Option<usize>),
    Other,
}

/// Reads declarations statement by statement, tracking what each brace
/// opens.
#[derive(Default)]
struct Reader {
    scopes: Vec<Scope>,
    /// A file-scoped namespace (`namespace Hello;`).
    file_namespace: String,
    statement: String,
    read: Vec<Declared>,
}

impl Reader {
    fn read(mut self, text: &str) -> Vec<Declared> {
        let mut string: Option<char> = None;
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            if let Some(quote) = string {
                self.statement.push(c);
                if c == '\\' {
                    self.statement.extend(chars.next());
                } else if c == quote {
                    string = None;
                }
                continue;
            }
            match c {
                '"' | '\'' => {
                    string = Some(c);
                    self.statement.push(c);
                }
                '{' => self.open(),
                '}' => {
                    self.scopes.pop();
                    self.statement.clear();
                }
                ';' => self.end_statement(),
                _ => self.statement.push(c),
            }
        }
        self.read
    }

    fn namespace(&self) -> String {
        let named: Vec<&str> = self
            .scopes
            .iter()
            .filter_map(|s| match s {
                Scope::Namespace(name) => Some(name.as_str()),
                _ => None,
            })
            .collect();
        if named.is_empty() { self.file_namespace.clone() } else { named.join(".") }
    }

    /// A brace opens a namespace, a type, or anything else.
    fn open(&mut self) {
        let header = without_attributes(&std::mem::take(&mut self.statement)).0;
        let words: Vec<&str> = header.split_whitespace().collect();
        let scope = if let Some(at) = words.iter().position(|w| *w == "namespace") {
            Scope::Namespace(words.get(at + 1).copied().unwrap_or_default().to_string())
        } else if let Some(at) =
            words.iter().position(|w| matches!(*w, "class" | "struct" | "record" | "interface" | "enum"))
        {
            let inside_type = self.scopes.iter().any(|s| matches!(s, Scope::Type(_)));
            let readable = words[at] == "class" && !inside_type && !header.contains('<');
            Scope::Type(readable.then(|| self.declare(&header, &words, at)))
        } else {
            Scope::Other
        };
        self.scopes.push(scope);
    }

    fn declare(&mut self, header: &str, words: &[&str], at: usize) -> usize {
        let name = words.get(at + 1).map(|w| w.trim_end_matches(':')).unwrap_or_default();
        let base = header.split_once(':').and_then(|(_, bases)| {
            let first = bases.split([',', ' ', '\n', '\t']).find(|part| !part.is_empty())?;
            Some(first.trim().to_string())
        });
        let namespace = self.namespace();
        let full = if namespace.is_empty() { name.to_string() } else { format!("{namespace}.{name}") };
        let is_abstract = words[..at].contains(&"abstract");
        self.read.push(Declared { name: full, namespace, base, is_abstract, fields: Vec::new() });
        self.read.len() - 1
    }

    fn end_statement(&mut self) {
        let statement = std::mem::take(&mut self.statement);
        match self.scopes.last() {
            None if statement.trim_start().starts_with("namespace ") => {
                self.file_namespace = statement.trim().trim_start_matches("namespace").trim().to_string();
            }
            Some(Scope::Type(Some(index))) => {
                let index = *index;
                if let Some(field) = field(&statement) {
                    self.read[index].fields.push(field);
                }
            }
            _ => {}
        }
    }
}

/// A statement without its leading `[...]` attributes, and their names.
fn without_attributes(statement: &str) -> (String, Vec<String>) {
    let mut rest = statement.trim();
    let mut names = Vec::new();
    while let Some(inner) = rest.strip_prefix('[') {
        let Some(end) = inner.find(']') else { break };
        for attribute in inner[..end].split(',') {
            let name = attribute.split('(').next().unwrap_or_default().trim();
            names.push(name.trim_end_matches("Attribute").to_string());
        }
        rest = inner[end + 1..].trim_start();
    }
    (rest.to_string(), names)
}

/// The field a member statement declares, if it is one a scene can set.
fn field(statement: &str) -> Option<ScriptField> {
    let (declaration, attributes) = without_attributes(statement);
    let (left, value) = match declaration.split_once('=') {
        Some((left, value)) => (left, Some(value.trim())),
        None => (declaration.as_str(), None),
    };
    // Methods, delegates, events, indexers and several fields at once are not single fields.
    if left.contains('(') || left.contains(',') || value.is_some_and(|v| v.starts_with('>')) {
        return None;
    }
    let words: Vec<&str> = left.split_whitespace().collect();
    let (&name, before) = words.split_last()?;
    const MODIFIERS: [&str; 9] =
        ["public", "private", "protected", "internal", "new", "required", "volatile", "unsafe", "override"];
    let modifiers: Vec<&str> = before.iter().copied().take_while(|w| MODIFIERS.contains(w) || is_excluded(w)).collect();
    let type_name = before[modifiers.len()..].join(" ");
    let settable = modifiers.contains(&"public") || attributes.iter().any(|a| a == "SerializeField");
    let excluded = modifiers.iter().any(|w| is_excluded(w)) || attributes.iter().any(|a| a == "NonSerialized");
    if !settable || excluded || type_name.is_empty() || matches!(type_name.as_str(), "event" | "delegate") {
        return None;
    }
    let is_name = |w: &str| w.chars().all(|c| c.is_alphanumeric() || c == '_');
    is_name(name).then(|| ScriptField {
        name: name.to_string(),
        type_name: type_name.clone(),
        default: value.and_then(|v| literal(v, &type_name)),
    })
}

fn is_excluded(word: &str) -> bool {
    matches!(word, "static" | "const" | "readonly" | "event" | "abstract")
}

/// A literal as a scene would write it: `90f` is 90.0, `"text"` is text.
fn literal(text: &str, type_name: &str) -> Option<toml::Value> {
    let text = text.trim();
    match text {
        "true" => return Some(toml::Value::Boolean(true)),
        "false" => return Some(toml::Value::Boolean(false)),
        _ => {}
    }
    if let Some(inner) = text.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
        return Some(toml::Value::String(inner.replace("\\\"", "\"")));
    }
    let number = text.trim_end_matches(['f', 'F', 'd', 'D', 'm', 'M']).replace('_', "");
    match type_name.trim_end_matches('?') {
        "int" | "long" | "short" | "byte" | "uint" | "ulong" | "ushort" | "sbyte" => {
            number.parse::<i64>().ok().map(toml::Value::Integer)
        }
        "float" | "double" | "decimal" => number.parse::<f64>().ok().map(toml::Value::Float),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPINNER: &str = r#"
using Voxear;

namespace Hello
{
    /// <summary>Spins { its Obj }.</summary>
    public sealed class Spinner : Behaviour
    {
        [SerializeField] private float degreesPerSecond = 90f; // turns { per second
        [SerializeField] private Spinner? leader;
        public string Label = "a \"spinner\" {";
        public bool Loud;
        [NonSerialized] public int Hidden = 3;
        public static int Count;
        private float timer;
        public float Speed => degreesPerSecond;
        public int Score { get; private set; }

        protected override void Update()
        {
            var x = "}";
            timer += 1f;
        }
    }

    public abstract class Enemy : Behaviour
    {
        public int Health = 10;
    }

    public class Boss : Enemy
    {
        [SerializeField, Tooltip("how hard")] private float rage = 0.5f;
    }

    [Serializable]
    public struct LevelStop
    {
        public string Level;
    }

    public class Helper
    {
        public int Value;
    }
}
"#;

    #[test]
    fn behaviours_and_their_settable_fields_are_read_from_source() {
        let types =
            scan(&[SPINNER, "namespace Other;\npublic class Plain : Voxear.Behaviour { public long Big = 1_000; }"]);
        let names: Vec<&str> = types.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Hello.Boss", "Hello.Spinner", "Other.Plain"], "not abstract classes, structs or helpers");

        let spinner = &types[1];
        let fields: Vec<(&str, &str)> =
            spinner.fields.iter().map(|f| (f.name.as_str(), f.type_name.as_str())).collect();
        assert_eq!(
            fields,
            [("degreesPerSecond", "float"), ("leader", "Spinner?"), ("Label", "string"), ("Loud", "bool")]
        );
        assert_eq!(spinner.fields[0].default, Some(toml::Value::Float(90.0)));
        assert_eq!(spinner.fields[2].default, Some(toml::Value::String("a \"spinner\" {".into())));
        assert_eq!(spinner.field("degrees_per_second").map(|f| f.name.as_str()), Some("degreesPerSecond"));

        let boss: Vec<&str> = types[0].fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(boss, ["Health", "rage"], "the base class's fields come first");
        assert_eq!(types[2].fields[0].default, Some(toml::Value::Integer(1000)));
    }

    #[test]
    fn hellos_behaviours_are_found() {
        let hello = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/Hello");
        let types = behaviour_types(&Project::open(&hello).unwrap());
        let names: Vec<&str> = types.iter().map(BehaviourType::short_name).collect();
        for expected in ["FireflySpawner", "GameManager", "Pulse", "Spinner"] {
            assert!(names.contains(&expected), "{expected} in {names:?}");
        }
        let spawner = types.iter().find(|t| t.short_name() == "FireflySpawner").unwrap();
        let fields: Vec<&str> = spawner.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(fields, ["firefly", "rareFirefly", "lamps", "interval", "spread", "rareChance"]);
    }
}
