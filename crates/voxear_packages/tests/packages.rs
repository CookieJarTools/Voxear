//! Packages end to end, against Git repositories made in a temporary folder:
//! fetching, resolving through a registry, locking, updating, adding and
//! removing, and publishing.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use semver::Version;
use voxear_packages::{
    Add, Cache, Change, GitRev, Lock, Options, Origin, Update, authoring, plan_add, plan_remove, sync,
};

fn git(folder: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(folder)
        .args(["-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false", "-c", "core.hooksPath=/dev/null"])
        .args(args)
        .env("GIT_AUTHOR_NAME", "Tests")
        .env("GIT_AUTHOR_EMAIL", "tests@example.com")
        .env("GIT_COMMITTER_NAME", "Tests")
        .env("GIT_COMMITTER_EMAIL", "tests@example.com")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn write(folder: &Path, file: &str, text: &str) {
    let path = folder.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn repository(folder: &Path) -> PathBuf {
    std::fs::create_dir_all(folder).unwrap();
    git(folder, &["init", "--quiet", "--initial-branch=main"]);
    folder.to_path_buf()
}

/// Commits everything in a repository and tags it; returns the commit.
fn release(folder: &Path, tag: &str) -> String {
    git(folder, &["add", "-A"]);
    git(folder, &["commit", "--quiet", "-m", tag]);
    git(folder, &["tag", tag]);
    git(folder, &["rev-parse", "HEAD"])
}

fn manifest(name: &str, version: &str, dependencies: &str) -> String {
    format!(
        "[package]\nname = \"{name}\"\nversion = \"{version}\"\nengine = \">=0.1\"\n\n[dependencies]\n{dependencies}"
    )
}

fn url(folder: &Path) -> String {
    folder.to_string_lossy().into_owned()
}

/// A registry repository listing releases, as `(name, version, repository, commit)`.
fn registry(folder: &Path, releases: &[(&str, &str, &Path, &str)]) {
    if !folder.join(".git").exists() {
        repository(folder);
    }
    for (name, version, repository, commit) in releases {
        let file = folder.join("packages").join(format!("{name}.toml"));
        let mut text =
            std::fs::read_to_string(&file).unwrap_or_else(|_| format!("description = \"{name} for tests\"\n"));
        text.push_str(&format!(
            "\n[[version]]\nversion = \"{version}\"\ngit = \"{}\"\ncommit = \"{commit}\"\n",
            url(repository)
        ));
        write(folder, &format!("packages/{name}.toml"), &text);
    }
    git(folder, &["add", "-A"]);
    git(folder, &["commit", "--quiet", "-m", "releases"]);
}

struct World {
    _temp: tempfile::TempDir,
    root: PathBuf,
    cache: Cache,
    springs: PathBuf,
    springs_commits: Vec<String>,
    orbit: PathBuf,
    registry: PathBuf,
    project: PathBuf,
}

/// Springs 0.1.0 and 0.2.0 in a registry, orbit-tools 1.0.0 in Git needing
/// springs ^0.2 from the registry, and a project asking for orbit-tools.
fn world() -> World {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_path_buf();
    let springs = repository(&root.join("springs"));
    write(&springs, "package.toml", &manifest("springs", "0.1.0", ""));
    write(&springs, "Scripts/Spring.cs", "class Spring {}");
    let first = release(&springs, "v0.1.0");
    write(&springs, "package.toml", &manifest("springs", "0.2.0", ""));
    let second = release(&springs, "v0.2.0");

    let orbit = repository(&root.join("orbit"));
    write(&orbit, "package.toml", &manifest("orbit-tools", "1.0.0", "springs = \"^0.2\"\n"));
    write(&orbit, "Docs/Orbit.md", "# Orbit");
    release(&orbit, "v1.0.0");

    let registry_folder = root.join("registry");
    registry(&registry_folder, &[("springs", "0.1.0", &springs, &first), ("springs", "0.2.0", &springs, &second)]);

    let project = root.join("game");
    write(
        &project,
        "packages.toml",
        &format!(
            "[packages]\norbit-tools = {{ git = \"{}\", tag = \"v1.0.0\" }}\n\n[registries]\ndefault = \"{}\"\n",
            url(&orbit),
            url(&registry_folder)
        ),
    );
    World {
        cache: Cache::at(root.join("cache")),
        _temp: temp,
        root,
        springs,
        springs_commits: vec![first, second],
        orbit,
        registry: registry_folder,
        project,
    }
}

fn options(world: &World, update: Update) -> Options<'_> {
    Options { engine: Version::new(0, 1, 0), cache: &world.cache, offline: false, update }
}

#[test]
fn a_project_resolves_through_git_and_a_registry() {
    let world = world();
    let packages = sync(&world.project, &options(&world, Update::Nothing)).unwrap();
    let names: Vec<&str> = packages.iter().map(|package| package.name.as_str()).collect();
    assert_eq!(names, ["springs", "orbit-tools"], "dependencies come first");
    assert_eq!(packages[0].version(), &Version::new(0, 2, 0));
    assert!(!packages[0].direct && packages[1].direct);
    assert!(packages[0].root.join("Scripts/Spring.cs").is_file());
    assert!(packages[0].root.starts_with(world.cache.root()));
    assert!(matches!(&packages[1].origin, Origin::Git { rev: GitRev::Tag(tag), .. } if tag == "v1.0.0"));

    let lock = Lock::read(&world.project).unwrap();
    assert_eq!(lock.packages.len(), 2);
    assert_eq!(lock.get("springs").unwrap().commit.as_deref(), Some(world.springs_commits[1].as_str()));

    // With the lock and the cache, nothing needs the network.
    let offline = Options { offline: true, ..options(&world, Update::Nothing) };
    let again = sync(&world.project, &offline).unwrap();
    assert_eq!(again.len(), 2);
    assert_eq!(Lock::read(&world.project).unwrap(), lock);
}

#[test]
fn the_lock_holds_until_an_update() {
    let world = world();
    sync(&world.project, &options(&world, Update::Nothing)).unwrap();

    write(&world.springs, "package.toml", &manifest("springs", "0.2.1", ""));
    let newer = release(&world.springs, "v0.2.1");
    registry(&world.registry, &[("springs", "0.2.1", &world.springs, &newer)]);

    let kept = sync(&world.project, &options(&world, Update::Nothing)).unwrap();
    assert_eq!(kept[0].version(), &Version::new(0, 2, 0));

    let plan = voxear_packages::plan(&world.project, &options(&world, Update::Only(vec!["springs".into()]))).unwrap();
    assert_eq!(
        plan.changes,
        [Change::Changed { name: "springs".into(), from: "0.2.0".into(), to: Version::new(0, 2, 1) }]
    );
    let updated = plan.apply().unwrap();
    assert_eq!(updated[0].version(), &Version::new(0, 2, 1));
    assert_eq!(Lock::read(&world.project).unwrap().get("springs").unwrap().commit.as_deref(), Some(newer.as_str()));
}

#[test]
fn adding_and_removing_say_what_changes_first() {
    let world = world();
    sync(&world.project, &options(&world, Update::Nothing)).unwrap();

    let tool = world.root.join("my-tool");
    write(&tool, "package.toml", &manifest("my-tool", "0.3.0", ""));
    let plan = plan_add(&world.project, Add::Folder(tool.clone()), &options(&world, Update::Nothing)).unwrap();
    let added: Vec<&str> = plan.added().map(|package| package.name.as_str()).collect();
    assert_eq!(added, ["my-tool"]);
    // Nothing is written until the plan is applied.
    assert!(!std::fs::read_to_string(world.project.join("packages.toml")).unwrap().contains("my-tool"));
    plan.apply().unwrap();
    let text = std::fs::read_to_string(world.project.join("packages.toml")).unwrap();
    assert!(text.contains("my-tool = { path = \"../my-tool\" }"), "{text}");

    let plan = plan_remove(&world.project, "orbit-tools", &options(&world, Update::Nothing)).unwrap();
    let mut removed: Vec<String> = plan
        .changes
        .iter()
        .filter_map(|change| match change {
            Change::Removed { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    removed.sort();
    assert_eq!(removed, ["orbit-tools", "springs"], "springs was only there for orbit-tools");
    let left = plan.apply().unwrap();
    assert_eq!(left.len(), 1);
    assert!(plan_remove(&world.project, "orbit-tools", &options(&world, Update::Nothing)).is_err());
}

#[test]
fn a_git_package_is_added_by_its_url_and_found_by_name_in_a_registry() {
    let world = world();
    std::fs::remove_file(world.project.join("packages.toml")).unwrap();
    write(&world.project, "packages.toml", &format!("[registries]\ndefault = \"{}\"\n", url(&world.registry)));
    let add = Add::Git { url: url(&world.orbit), rev: GitRev::Tag("v1.0.0".into()), path: None };
    let plan = plan_add(&world.project, add, &options(&world, Update::Nothing)).unwrap();
    assert_eq!(plan.added().count(), 2, "orbit-tools and the springs it needs");
    plan.apply().unwrap();

    let add = Add::Registry { name: "springs".into(), version: None, registry: None };
    plan_add(&world.project, add, &options(&world, Update::Nothing)).unwrap().apply().unwrap();
    let text = std::fs::read_to_string(world.project.join("packages.toml")).unwrap();
    assert!(text.contains("springs = \"^0.2.0\""), "{text}");

    let missing = Add::Registry { name: "nothing-here".into(), version: None, registry: None };
    let error = plan_add(&world.project, missing, &options(&world, Update::Nothing)).err().unwrap();
    assert!(format!("{error:#}").contains("does not list"), "{error:#}");
}

#[test]
fn needs_that_cannot_all_be_met_are_explained() {
    let world = world();
    let text = std::fs::read_to_string(world.project.join("packages.toml")).unwrap();
    let text = text.replace("[packages]\n", "[packages]\nsprings = \"^0.1\"\n");
    write(&world.project, "packages.toml", &text);
    let error = sync(&world.project, &options(&world, Update::Nothing)).err().unwrap();
    let message = format!("{error:#}");
    assert!(message.contains("no release of springs suits everyone"), "{message}");
    assert!(message.contains("the project needs ^0.1") && message.contains("orbit-tools needs ^0.2"), "{message}");
}

#[test]
fn a_package_under_the_wrong_name_is_refused() {
    let world = world();
    let text = format!("[packages]\ncameras = {{ git = \"{}\", tag = \"v1.0.0\" }}\n", url(&world.orbit));
    write(&world.project, "packages.toml", &text);
    let error = sync(&world.project, &options(&world, Update::Nothing)).err().unwrap();
    assert!(format!("{error:#}").contains("is called orbit-tools"), "{error:#}");
}

#[test]
fn a_tagged_release_is_published_to_a_registry() {
    let world = world();
    git(&world.orbit, &["remote", "add", "origin", &url(&world.orbit)]);
    let file = authoring::publish(&world.orbit, &world.registry).unwrap();
    let text = std::fs::read_to_string(file).unwrap();
    assert!(text.contains("version = \"1.0.0\"") && text.contains("tag = \"v1.0.0\""), "{text}");
    assert!(authoring::publish(&world.orbit, &world.registry).is_err(), "a version is listed once");

    write(&world.orbit, "Docs/More.md", "# More");
    let error = authoring::release_of(&world.orbit).err().unwrap();
    assert!(error.to_string().contains("not committed"), "{error}");
}

#[test]
fn a_package_moved_to_another_place_is_shown_again() {
    let world = world();
    let tool = world.root.join("my-tool");
    write(&tool, "package.toml", &manifest("my-tool", "0.3.0", ""));
    plan_add(&world.project, Add::Folder(tool.clone()), &options(&world, Update::Nothing)).unwrap().apply().unwrap();
    let kept = voxear_packages::plan(&world.project, &options(&world, Update::Nothing)).unwrap();
    assert_eq!(kept.arriving().count(), 0, "nothing new after a sync");

    repository(&tool);
    release(&tool, "v0.3.0");
    let add = Add::Git { url: url(&tool), rev: GitRev::Tag("v0.3.0".into()), path: None };
    let moved = plan_add(&world.project, add, &options(&world, Update::Nothing)).unwrap();
    let arriving: Vec<&str> = moved.arriving().map(|package| package.name.as_str()).collect();
    assert_eq!(arriving, ["my-tool"], "from a folder to Git is code from somewhere new");
    assert_eq!(moved.added().count(), 0);
}
