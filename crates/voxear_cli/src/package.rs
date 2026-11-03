//! `vox package`: adding, removing and updating a project's packages, and
//! starting and publishing packages of one's own.

use std::{
    io::{BufRead, IsTerminal, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use voxear_engine::{Project, engine_version};
use voxear_packages::{
    Add, Cache, Change, Contents, Fetch, GitRev, Options, Origin, PackagesFile, Plan, Update, authoring, plan_add,
    plan_remove, source::short,
};

#[derive(Subcommand)]
pub enum PackageCommand {
    /// Add a package: by a Git URL, a folder, or a name from a registry (name or name@range).
    Add {
        /// A Git URL, such as https://github.com/someone/orbit-tools, a package's folder, or a registry name.
        package: String,
        /// The Git tag to use.
        #[arg(long, conflicts_with_all = ["branch", "rev"])]
        tag: Option<String>,
        /// The Git branch to follow.
        #[arg(long, conflicts_with = "rev")]
        branch: Option<String>,
        /// The exact Git commit to use.
        #[arg(long)]
        rev: Option<String>,
        /// The package's folder inside a repository that holds more than one.
        #[arg(long)]
        path: Option<String>,
        /// The registry to find it in, as packages.toml names it under [registries].
        #[arg(long)]
        registry: Option<String>,
        /// Add packages that run code without asking first.
        #[arg(long, short)]
        yes: bool,
        #[command(flatten)]
        project: ProjectArgs,
    },
    /// Stop using a package, with any that only it needed.
    Remove {
        name: String,
        #[command(flatten)]
        project: ProjectArgs,
    },
    /// Move packages to their newest commits and releases that fit packages.toml: all of them, or those named.
    Update {
        names: Vec<String>,
        /// Take new packages that run code without asking first.
        #[arg(long, short)]
        yes: bool,
        #[command(flatten)]
        project: ProjectArgs,
    },
    /// List the packages the project uses, fetching any the machine does not have.
    List {
        #[command(flatten)]
        project: ProjectArgs,
    },
    /// Search a registry for packages by words in their names and descriptions.
    Search {
        words: Vec<String>,
        /// The registry to search, as packages.toml names it under [registries].
        #[arg(long)]
        registry: Option<String>,
        #[command(flatten)]
        project: ProjectArgs,
    },
    /// Start a package in a new folder.
    New {
        folder: PathBuf,
        /// Its name (default: the folder's, in lower case with hyphens).
        #[arg(long)]
        name: Option<String>,
    },
    /// List a package's tagged release in a registry: write it into a clone of the registry, to commit and push.
    Publish {
        /// A clone of the registry's repository.
        #[arg(long)]
        registry: PathBuf,
        /// The package's folder.
        #[arg(default_value = ".")]
        package: PathBuf,
    },
}

#[derive(clap::Args)]
pub struct ProjectArgs {
    /// Project folder or its voxear.toml.
    #[arg(long, default_value = ".")]
    project: PathBuf,
    /// Use only what is on this machine already, fetching nothing.
    #[arg(long)]
    offline: bool,
}

pub fn run(command: PackageCommand) -> Result<()> {
    let cache = Cache::for_machine();
    match command {
        PackageCommand::Add { package, tag, branch, rev, path, registry, yes, project } => {
            let rev = match (tag, branch, rev) {
                (Some(tag), _, _) => GitRev::Tag(tag),
                (_, Some(branch), _) => GitRev::Branch(branch),
                (_, _, Some(rev)) => GitRev::Commit(rev),
                _ => GitRev::Default,
            };
            let add = what_to_add(&package, rev, path, registry)?;
            let root = project_root(&project.project)?;
            let options = options(&cache, project.offline, Update::Nothing);
            let plan = plan_add(&root, add, &options)?;
            apply(plan, yes)
        }
        PackageCommand::Remove { name, project } => {
            let root = project_root(&project.project)?;
            let plan = plan_remove(&root, &name, &options(&cache, project.offline, Update::Nothing))?;
            apply(plan, true)
        }
        PackageCommand::Update { names, yes, project } => {
            let root = project_root(&project.project)?;
            let update = if names.is_empty() { Update::All } else { Update::Only(names) };
            let plan = voxear_packages::plan(&root, &options(&cache, project.offline, update))?;
            apply(plan, yes)
        }
        PackageCommand::List { project } => {
            let root = project_root(&project.project)?;
            let packages = voxear_packages::sync(&root, &options(&cache, project.offline, Update::Nothing))?;
            if packages.is_empty() {
                println!("The project uses no packages. Add one with `vox package add`.");
            }
            for package in packages.iter().rev() {
                let needed = if package.direct { "" } else { " (needed by others)" };
                println!("{} {}, {}{needed}", package.name, package.version(), place(&package.origin));
            }
            Ok(())
        }
        PackageCommand::Search { words, registry, project } => {
            let root = project_root(&project.project)?;
            let registries = PackagesFile::open(&root)?.registries()?;
            let registry = registries.get(registry.as_deref())?;
            let fetch = if project.offline { Fetch::Offline } else { Fetch::Always };
            let found = registry.search(&cache, &words.join(" "), fetch)?;
            let engine = engine();
            if found.is_empty() {
                println!("Nothing in {} matches.", registry.url);
            }
            for listing in found {
                let latest = match listing.latest(&engine) {
                    Some(release) => release.version.to_string(),
                    None => "nothing for this engine".to_string(),
                };
                println!("{} {latest}: {}", listing.name, listing.description);
            }
            Ok(())
        }
        PackageCommand::New { folder, name } => {
            let name = match name {
                Some(name) => name,
                None => package_name(&folder)?,
            };
            authoring::new_package(&folder, &name, &engine())?;
            let shown = folder.display().to_string();
            let argument = if shown.contains(' ') { format!("\"{shown}\"") } else { shown.clone() };
            println!("Started {name} in {shown}. Add it to a project with `vox package add {argument}`.");
            Ok(())
        }
        PackageCommand::Publish { registry, package } => {
            let file = authoring::publish(&package, &registry)?;
            println!("Listed it in {}. Commit that and push it, or send it as a pull request.", file.display());
            Ok(())
        }
    }
}

/// A URL is Git, a folder that exists is a folder, and anything else is a
/// name from a registry, with an optional `@range`.
fn what_to_add(package: &str, rev: GitRev, path: Option<String>, registry: Option<String>) -> Result<Add> {
    let git = package.contains("://") || package.starts_with("git@") || package.ends_with(".git");
    if git {
        return Ok(Add::Git { url: package.to_string(), rev, path });
    }
    if Path::new(package).join(voxear_packages::MANIFEST_FILE).is_file() {
        return Ok(Add::Folder(PathBuf::from(package)));
    }
    if Path::new(package).is_dir() {
        bail!("{package} is a folder, but not a package: it has no {}", voxear_packages::MANIFEST_FILE);
    }
    let (name, version) = match package.split_once('@') {
        Some((name, range)) => {
            let range =
                semver::VersionReq::parse(range).with_context(|| format!("`{range}` is not a version range"))?;
            (name, Some(range))
        }
        None => (package, None),
    };
    Ok(Add::Registry { name: name.to_string(), version, registry })
}

fn apply(plan: Plan, yes: bool) -> Result<()> {
    if plan.changes.is_empty() {
        println!("Nothing changes.");
        return Ok(());
    }
    let mut code = false;
    for package in plan.arriving() {
        let contents = Contents::of(&package.root);
        code |= contents.runs_code() && !package.is_local();
        println!("+ {} {}, {}", package.name, package.version(), place(&package.origin));
        if !package.manifest.description.is_empty() {
            println!("    {}", package.manifest.description);
        }
        println!("    {}", holds(&contents));
    }
    for change in &plan.changes {
        match change {
            Change::Added { .. } => {}
            Change::Changed { name, .. } if plan.arriving().any(|package| package.name == *name) => {}
            Change::Removed { name, version } => println!("- {name} {version}"),
            Change::Changed { name, from, to } if from == &to.to_string() => {
                println!("~ {name} {to}, from another commit")
            }
            Change::Changed { name, from, to } => println!("~ {name} {from} to {to}"),
        }
    }
    if code && !yes && !confirm("These packages run code of their own when the project's scripts do. Add them?")? {
        println!("Nothing was changed.");
        return Ok(());
    }
    let packages = plan.apply()?;
    println!("The project uses {} package{}.", packages.len(), if packages.len() == 1 { "" } else { "s" });
    Ok(())
}

fn holds(contents: &Contents) -> String {
    let mut parts = Vec::new();
    let count = |count: usize, one: &str, many: &str| format!("{count} {}", if count == 1 { one } else { many });
    if !contents.scripts.is_empty() {
        parts.push(count(contents.scripts.len(), "script", "scripts"));
    }
    if contents.assets > 0 {
        parts.push(count(contents.assets, "asset", "assets"));
    }
    if contents.docs > 0 {
        parts.push(count(contents.docs, "page of docs", "pages of docs"));
    }
    if !contents.samples.is_empty() {
        parts.push(count(contents.samples.len(), "sample", "samples"));
    }
    if parts.is_empty() { "Nothing in it yet".to_string() } else { parts.join(", ") }
}

fn place(origin: &Origin) -> String {
    match origin {
        Origin::Git { url, commit, .. } => format!("{url} at {}", short(commit)),
        Origin::Folder { folder, .. } => folder.display().to_string(),
        Origin::Registry { registry, .. } => format!("from {registry}"),
    }
}

fn confirm(question: &str) -> Result<bool> {
    if !std::io::stdin().is_terminal() {
        bail!("{question} Run with --yes to say so without being asked");
    }
    print!("{question} [y/N] ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "Yes"))
}

fn options(cache: &Cache, offline: bool, update: Update) -> Options<'_> {
    Options { engine: engine(), cache, offline, update }
}

fn engine() -> semver::Version {
    semver::Version::parse(engine_version()).unwrap_or_else(|_| semver::Version::new(0, 1, 0))
}

fn project_root(path: &Path) -> Result<PathBuf> {
    Ok(Project::open(path)?.root)
}

/// A folder's name as a package name: `Orbit Tools` is `orbit-tools`.
fn package_name(folder: &Path) -> Result<String> {
    let name = folder.file_name().context("name the package with --name")?.to_string_lossy();
    let mut words = Vec::new();
    let mut word = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            if c.is_ascii_uppercase()
                && !word.is_empty()
                && word.chars().last().is_some_and(|last| last.is_ascii_lowercase())
            {
                words.push(std::mem::take(&mut word));
            }
            word.push(c.to_ascii_lowercase());
        } else if !word.is_empty() {
            words.push(std::mem::take(&mut word));
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    Ok(words.join("-"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_names_become_package_names() {
        assert_eq!(package_name(Path::new("tools/Orbit Tools")).unwrap(), "orbit-tools");
        assert_eq!(package_name(Path::new("OrbitTools")).unwrap(), "orbit-tools");
        assert_eq!(package_name(Path::new("springs_2d")).unwrap(), "springs-2d");
    }

    #[test]
    fn what_is_added_is_told_apart() {
        let git = what_to_add("https://github.com/someone/orbit-tools", GitRev::Default, None, None).unwrap();
        assert!(matches!(git, Add::Git { .. }));
        let named = what_to_add("cameras@^1.2", GitRev::Default, None, None).unwrap();
        assert!(matches!(named, Add::Registry { name, version: Some(_), .. } if name == "cameras"));
    }
}
