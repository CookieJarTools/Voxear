//! What the Package Manager window shows and does, apart from how (see
//! `package_manager`): the project's packages as rows, which of them have
//! updates, the changes worked out as a [`Plan`] with what each package
//! arriving holds, and the registry's packages for a search.
//!
//! Everything here may read Git and the network, which can take seconds, so
//! [`run`] does it on a thread of its own and sends back what it found.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::mpsc::Sender,
};

use anyhow::{Context, Result};
use voxear_engine::packages::{
    self, Add, Cache, Change, Contents, Fetch, GitRev, Origin, Package, PackagesFile, Plan, Update,
    contents::{DOCS_FOLDER, Sample},
    source::short,
    title,
};

/// The most of a README the window shows; the rest is in the file.
const README_MOST: usize = 6000;

/// Where a package comes from, as the window names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Git,
    Folder,
    Registry,
}

impl Source {
    pub fn of(origin: &Origin) -> Self {
        match origin {
            Origin::Git { .. } => Self::Git,
            Origin::Folder { .. } => Self::Folder,
            Origin::Registry { .. } => Self::Registry,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Git => "Git",
            Self::Folder => "Folder",
            Self::Registry => "Registry",
        }
    }
}

/// Which commit of a repository the Git URL form asks for, in the order its
/// dropdown lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevKind {
    Default,
    Tag,
    Branch,
    Revision,
}

impl RevKind {
    pub const ALL: [Self; 4] = [Self::Default, Self::Tag, Self::Branch, Self::Revision];

    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "Default branch",
            Self::Tag => "Tag",
            Self::Branch => "Branch",
            Self::Revision => "Revision",
        }
    }

    /// The revision, with the tag, branch or commit typed for it. An empty
    /// one is the default branch, as a URL with nothing after it is.
    pub fn rev(self, value: &str) -> GitRev {
        let value = value.trim().to_string();
        match self {
            _ if value.is_empty() => GitRev::Default,
            Self::Default => GitRev::Default,
            Self::Tag => GitRev::Tag(value),
            Self::Branch => GitRev::Branch(value),
            Self::Revision => GitRev::Commit(value),
        }
    }
}

/// A package the project uses, as the list and the details show it.
#[derive(Clone, Debug)]
pub struct Row {
    pub name: String,
    pub title: String,
    pub version: String,
    pub source: Source,
    /// The repository, folder or registry, with the tag, branch or commit.
    pub place: String,
    pub description: String,
    pub authors: Vec<String>,
    /// Whether the project asks for it itself; one only other packages need
    /// goes when they do, so it has no Remove.
    pub direct: bool,
    pub dependencies: Vec<String>,
    pub contents: Contents,
    pub readme: Option<String>,
    /// Its VoxDocs pages' titles.
    pub docs: Vec<String>,
    pub root: PathBuf,
}

impl Row {
    pub fn of(package: &Package) -> Self {
        let manifest = &package.manifest;
        Self {
            name: package.name.clone(),
            title: title(&package.name),
            version: manifest.version.to_string(),
            source: Source::of(&package.origin),
            place: place(&package.origin),
            description: manifest.description.clone(),
            authors: manifest.authors.clone(),
            direct: package.direct,
            dependencies: package.dependencies.clone(),
            contents: Contents::of(&package.root),
            readme: readme(&package.root),
            docs: doc_titles(&package.root),
            root: package.root.clone(),
        }
    }

    /// Whether every word of a search is in its name, title or description.
    pub fn matches(&self, search: &str) -> bool {
        let text = format!("{} {} {}", self.name, self.title, self.description).to_lowercase();
        search.split_whitespace().all(|word| text.contains(&word.to_lowercase()))
    }
}

/// A newer release or commit that fits what the project asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Available {
    pub version: String,
    /// The same version from a newer commit, as a followed branch moves.
    pub commit_only: bool,
}

impl Available {
    /// What the update is, short, as the list and the Update button say it.
    pub fn label(&self) -> String {
        if self.commit_only { "New commit".to_string() } else { self.version.clone() }
    }
}

/// A package a change brings, as the consent dialog shows it.
#[derive(Clone, Debug)]
pub struct Arrival {
    pub name: String,
    pub version: String,
    pub source: Source,
    pub place: String,
    pub description: String,
    pub holds: String,
    pub runs_code: bool,
    pub local: bool,
}

/// What a change was asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intent {
    Add,
    Remove,
    Update,
}

/// A change worked out but not written, with what it would do.
pub struct Planned {
    pub plan: Plan,
    pub intent: Intent,
    pub arriving: Vec<Arrival>,
    /// Packages that would go: name and version.
    pub removed: Vec<(String, String)>,
    /// Packages that would move, apart from those arriving: name, from, to.
    pub moved: Vec<(String, String, String)>,
}

impl Planned {
    fn of(plan: Plan, intent: Intent) -> Self {
        let arriving: Vec<Arrival> = plan
            .arriving()
            .map(|package| {
                let contents = Contents::of(&package.root);
                Arrival {
                    name: package.name.clone(),
                    version: package.version().to_string(),
                    source: Source::of(&package.origin),
                    place: place(&package.origin),
                    description: package.manifest.description.clone(),
                    holds: holds(&contents),
                    runs_code: contents.runs_code(),
                    local: package.is_local(),
                }
            })
            .collect();
        let mut removed = Vec::new();
        let mut moved = Vec::new();
        for change in &plan.changes {
            match change {
                Change::Removed { name, version } => removed.push((name.clone(), version.clone())),
                Change::Changed { name, .. } if arriving.iter().any(|arrival| arrival.name == *name) => {}
                Change::Changed { name, from, to } => moved.push((name.clone(), from.clone(), to.to_string())),
                Change::Added { .. } => {}
            }
        }
        Self { plan, intent, arriving, removed, moved }
    }

    /// Whether a person should see the change before it is written: it
    /// brings code from somewhere other than a folder of their own.
    pub fn asks_first(&self) -> bool {
        self.arriving.iter().any(|arrival| arrival.runs_code && !arrival.local)
    }

    pub fn changes_nothing(&self) -> bool {
        self.plan.changes.is_empty()
    }

    /// What was done, for the window's status line once it is written.
    pub fn done(&self) -> String {
        let count = self.plan.changes.len();
        let packages = if count == 1 { "1 package".to_string() } else { format!("{count} packages") };
        let first = |verb: &str| match (&self.arriving[..], count) {
            ([arrival], 1) => format!("{verb} {} {}", arrival.name, arrival.version),
            _ => format!("{verb} {packages}"),
        };
        match self.intent {
            Intent::Add => first("Added"),
            Intent::Remove => match (&self.removed[..], count) {
                ([(name, version)], 1) => format!("Removed {name} {version}"),
                _ => format!("Removed {packages}"),
            },
            Intent::Update => match (&self.moved[..], count) {
                ([(name, _, to)], 1) => format!("Updated {name} to {to}"),
                _ => format!("Updated {packages}"),
            },
        }
    }
}

/// A package a registry lists, as the registry's search shows it.
#[derive(Clone, Debug)]
pub struct Found {
    pub name: String,
    /// The newest release that works with this engine, if any does.
    pub latest: Option<String>,
    pub description: String,
}

/// A change to the project's packages.
#[derive(Clone, Debug)]
pub enum Asked {
    Add(Add),
    Remove(String),
    Update(Update),
}

/// Work for the window's thread.
#[derive(Clone, Debug)]
pub enum Task {
    /// What the project uses, then whether newer commits and releases fit.
    List,
    Change(Asked),
    Import {
        package: String,
        sample: Sample,
    },
    /// The registry's packages that hold every word, fetching the registry
    /// first when `fetch`.
    Search {
        words: String,
        fetch: bool,
    },
}

impl Task {
    /// What the window says while it runs.
    pub fn doing(&self) -> String {
        match self {
            Self::List => "Reading packages".to_string(),
            Self::Change(Asked::Add(Add::Git { url, .. })) => format!("Fetching {url}"),
            Self::Change(Asked::Add(Add::Folder(folder))) => format!("Reading {}", folder.display()),
            Self::Change(Asked::Add(Add::Registry { name, .. })) => format!("Finding {name}"),
            Self::Change(Asked::Remove(name)) => format!("Removing {name}"),
            Self::Change(Asked::Update(Update::Only(names))) => format!("Updating {}", names.join(", ")),
            Self::Change(Asked::Update(_)) => "Updating packages".to_string(),
            Self::Import { sample, .. } => format!("Importing {}", sample.name),
            Self::Search { .. } => "Searching the registry".to_string(),
        }
    }
}

/// What the thread found, sent as it goes.
pub enum Reply {
    /// What it is doing now, as the progress line says it.
    Doing(String),
    Listed(Vec<Row>),
    Updates(BTreeMap<String, Available>),
    Planned(Box<Planned>),
    Imported(PathBuf),
    Found(Vec<Found>),
    Failed(String),
}

/// Does a task, sending what it finds. Blocks for as long as Git takes.
pub fn run(task: Task, project: &Path, cache: &Cache, send: &Sender<Reply>) {
    let failed = |error: anyhow::Error| Reply::Failed(format!("{error:#}"));
    let reply = match task {
        Task::List => {
            match list(project, cache) {
                Ok(rows) => {
                    let _ = send.send(Reply::Listed(rows));
                }
                Err(error) => {
                    let _ = send.send(failed(error));
                    return;
                }
            }
            let _ = send.send(Reply::Doing("Checking for updates".to_string()));
            updates(project, cache).map(Reply::Updates)
        }
        Task::Change(asked) => change(project, cache, asked).map(|planned| Reply::Planned(Box::new(planned))),
        Task::Import { package, sample } => sample.import(project, &package).map(Reply::Imported),
        Task::Search { words, fetch } => search(project, cache, &words, fetch).map(Reply::Found),
    }
    .unwrap_or_else(failed);
    let _ = send.send(reply);
}

/// The packages the project uses as its files say, from what this machine
/// has when it has them all, and fetching only what it lacks when not.
/// Nothing is written: the list shows what is there.
fn list(project: &Path, cache: &Cache) -> Result<Vec<Row>> {
    let plan = match packages::plan(project, &packages::options(cache, true, Update::Nothing)) {
        Ok(plan) => plan,
        Err(_) => packages::plan(project, &packages::options(cache, false, Update::Nothing))?,
    };
    let mut rows: Vec<Row> = plan.packages.iter().map(Row::of).collect();
    rows.sort_by_key(|row| row.title.to_lowercase());
    Ok(rows)
}

/// The packages an Update All would move, by name: to newer releases that
/// fit, or newer commits on the branches they follow.
fn updates(project: &Path, cache: &Cache) -> Result<BTreeMap<String, Available>> {
    let plan =
        packages::plan(project, &packages::options(cache, false, Update::All)).context("checking for updates")?;
    Ok(available(&plan.changes))
}

/// The updates in a plan's changes, by name.
pub fn available(changes: &[Change]) -> BTreeMap<String, Available> {
    changes
        .iter()
        .filter_map(|change| match change {
            Change::Changed { name, from, to } => {
                let version = to.to_string();
                Some((name.clone(), Available { commit_only: *from == version, version }))
            }
            _ => None,
        })
        .collect()
}

fn change(project: &Path, cache: &Cache, asked: Asked) -> Result<Planned> {
    match asked {
        Asked::Add(add) => {
            let plan = packages::plan_add(project, add, &packages::options(cache, false, Update::Nothing))?;
            Ok(Planned::of(plan, Intent::Add))
        }
        Asked::Remove(name) => {
            let plan = packages::plan_remove(project, &name, &packages::options(cache, false, Update::Nothing))?;
            Ok(Planned::of(plan, Intent::Remove))
        }
        Asked::Update(update) => {
            let plan = packages::plan(project, &packages::options(cache, false, update))?;
            Ok(Planned::of(plan, Intent::Update))
        }
    }
}

fn search(project: &Path, cache: &Cache, words: &str, fetch: bool) -> Result<Vec<Found>> {
    let registries = PackagesFile::open(project)?.registries()?;
    let registry = &registries.default;
    let listings = match fetch {
        true => registry.search(cache, words, Fetch::Always)?,
        // Typing searches what was fetched when the search opened.
        false => registry
            .search(cache, words, Fetch::Offline)
            .or_else(|_| registry.search(cache, words, Fetch::IfMissing))?,
    };
    let engine = packages::engine();
    Ok(listings
        .into_iter()
        .map(|listing| Found {
            latest: listing.latest(&engine).map(|release| release.version.to_string()),
            name: listing.name,
            description: listing.description,
        })
        .collect())
}

/// Where a package comes from, as one line.
pub fn place(origin: &Origin) -> String {
    match origin {
        Origin::Git { url, rev, path, commit } => {
            let mut parts = vec![url.clone()];
            match rev {
                GitRev::Tag(tag) => parts.push(tag.clone()),
                GitRev::Branch(branch) => parts.extend([branch.clone(), short(commit).to_string()]),
                GitRev::Commit(_) | GitRev::Default => parts.push(short(commit).to_string()),
            }
            parts.extend(path.clone());
            parts.join(" · ")
        }
        Origin::Folder { asked, .. } => asked.display().to_string(),
        Origin::Registry { registry, .. } => registry.clone(),
    }
}

/// What a package holds, as one short line: "2 scripts, 3 assets, 1 page".
pub fn holds(contents: &Contents) -> String {
    let count = |count: usize, one: &str, many: &str| format!("{count} {}", if count == 1 { one } else { many });
    let mut parts = Vec::new();
    if !contents.scripts.is_empty() {
        parts.push(count(contents.scripts.len(), "script", "scripts"));
    }
    if contents.assets > 0 {
        parts.push(count(contents.assets, "asset", "assets"));
    }
    if contents.docs > 0 {
        parts.push(count(contents.docs, "page", "pages"));
    }
    if !contents.samples.is_empty() {
        parts.push(count(contents.samples.len(), "sample", "samples"));
    }
    if parts.is_empty() { "Empty".to_string() } else { parts.join(", ") }
}

/// The package's README, `README.md` or any other casing or extension.
fn readme(root: &Path) -> Option<String> {
    let entries = std::fs::read_dir(root).ok()?;
    let path = entries.flatten().map(|entry| entry.path()).find(|path| {
        path.is_file()
            && path.file_stem().and_then(|stem| stem.to_str()).is_some_and(|stem| stem.eq_ignore_ascii_case("readme"))
    })?;
    let text = std::fs::read_to_string(path).ok()?;
    let text = text.trim();
    if text.chars().count() <= README_MOST {
        return Some(text.to_string());
    }
    Some(format!("{}…", text.chars().take(README_MOST).collect::<String>()))
}

/// Each VoxDocs page's title: its first `# ` heading, or its file's name.
fn doc_titles(root: &Path) -> Vec<String> {
    fn walk(folder: &Path, titles: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(folder) else { return };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
        paths.sort();
        for path in paths {
            if path.file_name().is_some_and(|name| name.to_string_lossy().starts_with('.')) {
                continue;
            }
            if path.is_dir() {
                walk(&path, titles);
            } else if path.extension().is_some_and(|ext| ext == "md") {
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                let heading = text.lines().find_map(|line| line.strip_prefix("# ")).map(str::trim);
                let stem = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
                titles.push(heading.filter(|heading| !heading.is_empty()).map_or(stem, str::to_string));
            }
        }
    }
    let mut titles = Vec::new();
    walk(&root.join(DOCS_FOLDER), &mut titles);
    titles
}

#[cfg(test)]
mod tests {
    use semver::Version;

    use super::*;

    #[test]
    fn updates_are_changes_to_packages_already_used() {
        let changes = [
            Change::Changed { name: "cams".into(), from: "1.0.0".into(), to: Version::new(1, 2, 0) },
            Change::Changed { name: "springs".into(), from: "0.2.0".into(), to: Version::new(0, 2, 0) },
            Change::Added { name: "new".into(), version: Version::new(1, 0, 0) },
        ];
        let found = available(&changes);
        assert_eq!(found.len(), 2);
        assert_eq!(found["cams"].label(), "1.2.0");
        assert!(found["springs"].commit_only, "the same version from a newer commit");
    }

    #[test]
    fn a_package_holds_what_its_folders_do() {
        let none = Contents::default();
        assert_eq!(holds(&none), "Empty");
        let some = Contents { scripts: vec!["A.cs".into(), "B.cs".into()], docs: 1, assets: 0, samples: Vec::new() };
        assert_eq!(holds(&some), "2 scripts, 1 page");
    }

    #[test]
    fn an_empty_revision_is_the_default_branch() {
        assert_eq!(RevKind::Tag.rev("  "), GitRev::Default);
        assert_eq!(RevKind::Branch.rev("main"), GitRev::Branch("main".into()));
    }
}
