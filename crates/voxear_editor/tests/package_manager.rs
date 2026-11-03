//! The Package Manager, on projects made in a temporary folder: opened from
//! the Window menu, it lists a folder package with what it holds, shows its
//! details and imports its sample; a package from Git with scripts waits on
//! the dialog before anything is written; and a followed branch that moves
//! shows as an update that Update All takes. Packages are fetched into a
//! cache of each test's own. Skips itself without a GPU.

use std::{
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

use voxear_core::logging::LogCapture;
use voxear_editor::Editor;
use voxear_engine::{
    Project,
    packages::{Add, Cache, GitRev},
};
use voxear_render::{Renderer, RendererSettings};
use voxear_ui::{NodeId, Ui};

const SIZE: (u32, u32) = (1400, 860);

const LEVEL: &str = "format = 1\n\n[[obj]]\nid = 1\nname = \"Main Camera\"\n[obj.camera]\nfov = 60.0\n";

fn write(folder: &Path, file: &str, text: &str) {
    let path = folder.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

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

fn manifest(name: &str, version: &str) -> String {
    format!(
        "[package]\nname = \"{name}\"\nversion = \"{version}\"\ndescription = \"Cameras that orbit\"\n\
         authors = [\"Sam Rivers\"]\nengine = \">=0.1\"\n"
    )
}

/// A temporary folder holding a project, `game`, and anything else a test
/// makes beside it.
struct World {
    temp: tempfile::TempDir,
}

/// The package cache these tests share, never the machine's: named in
/// `VOXEAR_PACKAGE_CACHE` too, for the editor's content, which mounts the
/// packages through the machine's cache.
fn cache() -> PathBuf {
    static CACHE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    CACHE
        .get_or_init(|| {
            let folder = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap().keep();
            // SAFETY: set once, as the first test starts and before any test starts a thread of its own;
            // the others wait here for it.
            unsafe { std::env::set_var(voxear_engine::packages::cache::CACHE_VARIABLE, &folder) };
            folder
        })
        .clone()
}

impl World {
    fn new() -> Self {
        cache();
        let temp = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
        let game = temp.path().join("game");
        write(&game, "voxear.toml", "[project]\nname = \"Orbits\"\nstart_level = \"Levels/Main.level\"\n");
        write(&game, "Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Main.scene\"\n");
        write(&game, "Scenes/Main.scene", LEVEL);
        Self { temp }
    }

    fn path(&self, file: &str) -> PathBuf {
        self.temp.path().join(file)
    }

    fn game(&self) -> PathBuf {
        self.path("game")
    }

    /// The editor on the project, with packages fetched into the tests' own
    /// cache, or `None` without a GPU.
    fn open(&self) -> Option<(Renderer, Editor)> {
        let renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
            Ok(renderer) => renderer,
            Err(error) => {
                eprintln!("skipping: no usable GPU ({error:#})");
                return None;
            }
        };
        let mut renderer = renderer;
        let project = Project::open(&self.game()).unwrap();
        let mut editor = Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
        editor.package_manager().set_cache(Cache::at(cache()));
        frame(&mut editor, &mut renderer);
        Some((renderer, editor))
    }
}

fn frame(editor: &mut Editor, renderer: &mut Renderer) {
    editor.update(renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
}

/// Frames until `done`, which must come within a minute.
fn until(editor: &mut Editor, renderer: &mut Renderer, what: &str, done: impl Fn(&mut Editor) -> bool) {
    let started = Instant::now();
    while !done(editor) {
        assert!(started.elapsed() < Duration::from_secs(60), "{what} never happened");
        frame(editor, renderer);
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn find(editor: &Editor, name: &str) -> NodeId {
    editor.ui.find(editor.ui.root(), name).unwrap_or_else(|| panic!("no {name}"))
}

/// The texts showing under `node`, leaving out what is hidden.
fn texts(ui: &Ui, node: NodeId) -> Vec<String> {
    if !ui.is_visible(node) {
        return Vec::new();
    }
    let mut found = vec![ui.text(node)];
    for child in ui.children(node) {
        found.extend(texts(ui, child));
    }
    found.retain(|text| !text.is_empty());
    found
}

fn click(editor: &mut Editor, renderer: &mut Renderer, node: NodeId) {
    let [x, y, w, h] = editor.ui.rect(node);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    editor.ui.pointer_button(true);
    editor.ui.pointer_button(false);
    frame(editor, renderer);
}

fn with_class(ui: &Ui, node: NodeId, class: &str, found: &mut Vec<NodeId>) {
    if ui.has_class(node, class) {
        found.push(node);
    }
    for child in ui.children(node) {
        with_class(ui, child, class, found);
    }
}

/// The open menu's item that says `label`.
fn menu_item(ui: &Ui, label: &str) -> Option<NodeId> {
    let mut items = Vec::new();
    with_class(ui, ui.root(), "menu-item", &mut items);
    items.into_iter().find(|&item| ui.children(item).iter().any(|&part| ui.text(part) == label))
}

/// Opens the Package Manager as a person does: the top bar's Window button,
/// then Package Manager.
fn open_window(editor: &mut Editor, renderer: &mut Renderer) {
    let windows = find(editor, "windows");
    click(editor, renderer, windows);
    let item = menu_item(&editor.ui, "Package Manager").expect("the Window menu lists the Package Manager");
    click(editor, renderer, item);
    frame(editor, renderer);
}

fn shot(renderer: &mut Renderer, name: &str) {
    let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") else { return };
    let (width, height, pixels) = renderer.read_pixels().unwrap();
    let file = std::fs::File::create(Path::new(&dir).join(name)).unwrap();
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
}

#[test]
fn the_window_lists_a_folder_package_and_imports_its_sample() {
    let world = World::new();
    let orbit = world.path("orbit-tools");
    write(&orbit, "package.toml", &manifest("orbit-tools", "1.2.0"));
    write(
        &orbit,
        "README.md",
        "# Orbit Tools\n\nCameras that orbit, follow and frame.\n\n- `Orbit`, round a target\n- `Follow`, behind one\n",
    );
    write(&orbit, "Scripts/Orbit.cs", "class Orbit {}");
    write(&orbit, "Docs/Orbits.md", "# Orbit Cameras\n\nHow they turn.");
    write(&orbit, "Models/Rig.glb", "");
    write(&orbit, "Samples/First Orbit/Readme.txt", "Open me.");
    write(&world.game(), "packages.toml", "[packages]\norbit-tools = { path = \"../orbit-tools\" }\n");
    let Some((mut renderer, mut editor)) = world.open() else { return };
    assert!(has_pages(&editor, "orbit-tools"), "VoxDocs shows its pages from the start");

    open_window(&mut editor, &mut renderer);
    until(&mut editor, &mut renderer, "the packages being read", |editor| {
        let manager = editor.package_manager();
        !manager.rows().is_empty() && !manager.is_busy()
    });
    let rows = editor.package_manager().rows().to_vec();
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].name.as_str(), rows[0].version.as_str()), ("orbit-tools", "1.2.0"));
    assert!(editor.package_manager().updates().is_empty(), "a folder has no updates");
    frame(&mut editor, &mut renderer);
    let list = editor.package_manager().list();
    let shown = texts(&editor.ui, list);
    for text in ["Orbit Tools", "1.2.0", "Folder  ·  1 script, 1 asset, 1 page, 1 sample"] {
        assert!(shown.iter().any(|shown| shown == text), "{text} in {shown:?}");
    }
    assert_eq!(editor.ui.text(find(&editor, "packages-summary")), "1 package");

    // Picked, its details show what it is, holds and has to read.
    editor.ui.set_selection(list, &[0]);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let detail = texts(&editor.ui, find(&editor, "packages-detail"));
    for text in ["Orbit Tools", "Cameras that orbit", "Sam Rivers", "Orbit Cameras", "First Orbit", "Import", "Remove"]
    {
        assert!(detail.iter().any(|shown| shown == text), "{text} in {detail:?}");
    }
    assert!(detail.iter().any(|shown| shown.contains("follow and frame")), "the README in {detail:?}");
    shot(&mut renderer, "package-manager.png");

    let import = find(&editor, "packages-import-First Orbit");
    click(&mut editor, &mut renderer, import);
    let copied = world.game().join("Samples/Orbit Tools/First Orbit/Readme.txt");
    until(&mut editor, &mut renderer, "the sample being imported", |_| copied.is_file());
    until(&mut editor, &mut renderer, "the import finishing", |editor| !editor.package_manager().is_busy());
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.ui.text(find(&editor, "packages-import-First Orbit")), "Imported");

    // Remove takes it out of packages.toml with no dialog: nothing arrives.
    let remove = find(&editor, "packages-remove");
    click(&mut editor, &mut renderer, remove);
    until(&mut editor, &mut renderer, "the package going", |editor| {
        let manager = editor.package_manager();
        manager.rows().is_empty() && !manager.is_busy()
    });
    assert!(!editor.package_manager().is_asking());
    let written = std::fs::read_to_string(world.game().join("packages.toml")).unwrap();
    assert!(!written.contains("orbit-tools"), "{written}");
    assert!(editor.game.content.db().mounts().packages().is_empty(), "its assets go");
    assert!(!has_pages(&editor, "orbit-tools"), "and VoxDocs has its pages no more");

    // A folder of the developer's own runs code they wrote, so it comes in without asking either.
    editor.package_manager().add(Add::Folder(orbit));
    until(&mut editor, &mut renderer, "the folder being added", |editor| {
        let manager = editor.package_manager();
        !manager.rows().is_empty() && !manager.is_busy()
    });
    assert!(!editor.package_manager().is_asking());
    let written = std::fs::read_to_string(world.game().join("packages.toml")).unwrap();
    assert!(written.contains("orbit-tools = { path = \"../orbit-tools\" }"), "{written}");
    let mounted = editor.game.content.db().mounts().packages().iter().any(|mount| mount.name == "orbit-tools");
    assert!(mounted, "its assets are mounted again");
    assert!(editor.game.content.db().guid_of("Packages/orbit-tools/Models/Rig.glb").is_some());
    assert!(has_pages(&editor, "orbit-tools"), "and VoxDocs shows its pages");
}

fn has_pages(editor: &Editor, package: &str) -> bool {
    editor.docs().library().pages().any(|(_, page)| page.package() == Some(package))
}

/// A Git repository of a package with a script, tagged v1.0.0 on `main`.
fn git_package(world: &World) -> (String, PathBuf) {
    let repository = world.path("orbit-repository");
    std::fs::create_dir_all(&repository).unwrap();
    git(&repository, &["init", "--quiet", "--initial-branch=main"]);
    write(&repository, "package.toml", &manifest("orbit-tools", "1.0.0"));
    write(&repository, "Scripts/Orbit.cs", "class Orbit {}");
    git(&repository, &["add", "-A"]);
    git(&repository, &["commit", "--quiet", "-m", "first"]);
    git(&repository, &["tag", "v1.0.0"]);
    (repository.to_string_lossy().into_owned(), repository)
}

#[test]
fn a_package_with_scripts_from_git_waits_on_the_dialog() {
    let world = World::new();
    let (url, _) = git_package(&world);
    let Some((mut renderer, mut editor)) = world.open() else { return };
    open_window(&mut editor, &mut renderer);
    until(&mut editor, &mut renderer, "the packages being read", |editor| !editor.package_manager().is_busy());
    assert!(texts(&editor.ui, find(&editor, "packages-empty")).contains(&"No packages".to_string()));

    // The + menu's Git URL form, at a tag.
    let add = find(&editor, "packages-add");
    click(&mut editor, &mut renderer, add);
    for label in ["Add from Git URL…", "Add from Folder…", "Add from Registry…"] {
        assert!(menu_item(&editor.ui, label).is_some(), "{label} in the + menu");
    }
    let item = menu_item(&editor.ui, "Add from Git URL…").unwrap();
    click(&mut editor, &mut renderer, item);
    frame(&mut editor, &mut renderer);
    let field = find(&editor, "packages-git-url");
    assert!(editor.ui.is_visible(field), "the form shows");
    editor.ui.focus(field);
    editor.ui.text_input(&url);
    let rev = find(&editor, "packages-git-rev");
    editor.ui.set_selected_option(rev, 1);
    let value = find(&editor, "packages-git-value");
    editor.ui.focus(value);
    editor.ui.text_input("v1.0.0");
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "package-manager-git.png");
    let add = find(&editor, "packages-git-add");
    click(&mut editor, &mut renderer, add);
    until(&mut editor, &mut renderer, "the dialog", |editor| editor.package_manager().is_asking());
    frame(&mut editor, &mut renderer);

    let dialog = find(&editor, "packages-dialog");
    assert!(editor.ui.is_visible(dialog));
    let lines = texts(&editor.ui, find(&editor, "packages-dialog-lines"));
    for text in ["+", "orbit-tools", "1.0.0", "1 script", "Cameras that orbit"] {
        assert!(lines.iter().any(|line| line == text), "{text} in {lines:?}");
    }
    assert!(lines.iter().any(|line| line.starts_with("Git  ·  ") && line.ends_with("v1.0.0")), "from in {lines:?}");
    assert!(texts(&editor.ui, dialog).contains(&"Add".to_string()));
    shot(&mut renderer, "package-manager-dialog.png");
    assert!(!world.game().join("packages.toml").exists(), "nothing is written before Add");

    let cancel = find(&editor, "packages-dialog-cancel");
    click(&mut editor, &mut renderer, cancel);
    assert!(!editor.package_manager().is_asking() && !editor.ui.is_visible(dialog));
    assert!(!world.game().join("packages.toml").exists(), "nor after Cancel");

    // Asked again, Add writes it, and the list shows it.
    editor.package_manager().add(Add::Git { url, rev: GitRev::Tag("v1.0.0".into()), path: None });
    until(&mut editor, &mut renderer, "the dialog again", |editor| editor.package_manager().is_asking());
    frame(&mut editor, &mut renderer);
    let add = find(&editor, "packages-dialog-add");
    click(&mut editor, &mut renderer, add);
    until(&mut editor, &mut renderer, "the package being listed", |editor| {
        let manager = editor.package_manager();
        manager.rows().iter().any(|row| row.name == "orbit-tools") && !manager.is_busy()
    });
    let written = std::fs::read_to_string(world.game().join("packages.toml")).unwrap();
    assert!(written.contains("orbit-tools") && written.contains("v1.0.0"), "{written}");
    assert!(world.game().join("packages.lock").is_file());
}

#[test]
fn a_followed_branch_that_moves_is_an_update() {
    let world = World::new();
    let (url, repository) = git_package(&world);
    let Some((mut renderer, mut editor)) = world.open() else { return };
    open_window(&mut editor, &mut renderer);
    editor.package_manager().add(Add::Git { url, rev: GitRev::Branch("main".into()), path: None });
    until(&mut editor, &mut renderer, "the dialog", |editor| editor.package_manager().is_asking());
    frame(&mut editor, &mut renderer);
    let add = find(&editor, "packages-dialog-add");
    click(&mut editor, &mut renderer, add);
    until(&mut editor, &mut renderer, "the package being listed", |editor| {
        let manager = editor.package_manager();
        !manager.rows().is_empty() && !manager.is_busy()
    });
    assert!(editor.package_manager().updates().is_empty(), "the branch has not moved");

    write(&repository, "package.toml", &manifest("orbit-tools", "1.1.0"));
    git(&repository, &["commit", "--quiet", "-am", "newer"]);
    let refresh = find(&editor, "packages-refresh");
    click(&mut editor, &mut renderer, refresh);
    until(&mut editor, &mut renderer, "the update being found", |editor| {
        let manager = editor.package_manager();
        !manager.updates().is_empty() && !manager.is_busy()
    });
    assert_eq!(editor.package_manager().updates()["orbit-tools"].version, "1.1.0");
    frame(&mut editor, &mut renderer);
    let list = editor.package_manager().list();
    let shown = texts(&editor.ui, list);
    assert!(shown.contains(&"1.1.0".to_string()), "the list marks it: {shown:?}");
    editor.ui.set_selection(list, &[0]);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(texts(&editor.ui, find(&editor, "packages-update")).contains(&"Update to 1.1.0".to_string()));
    shot(&mut renderer, "package-manager-update.png");

    // The same repository and branch bring no new code, so it is taken without asking.
    let update_all = find(&editor, "packages-update-all");
    click(&mut editor, &mut renderer, update_all);
    until(&mut editor, &mut renderer, "the update", |editor| {
        let manager = editor.package_manager();
        manager.rows().first().is_some_and(|row| row.version == "1.1.0") && !manager.is_busy()
    });
    assert!(!editor.package_manager().is_asking());
    assert!(editor.package_manager().updates().is_empty());
}

#[test]
fn a_package_found_in_the_registry_is_added() {
    let world = World::new();
    let (url, repository) = git_package(&world);
    let commit = git(&repository, &["rev-parse", "HEAD"]);
    let registry = world.path("registry");
    std::fs::create_dir_all(&registry).unwrap();
    git(&registry, &["init", "--quiet", "--initial-branch=main"]);
    let listing = format!(
        "description = \"Cameras that orbit\"\n\n[[version]]\nversion = \"1.0.0\"\ngit = \"{url}\"\ncommit = \"{commit}\"\n"
    );
    write(&registry, "packages/orbit-tools.toml", &listing);
    git(&registry, &["add", "-A"]);
    git(&registry, &["commit", "--quiet", "-m", "releases"]);
    write(&world.game(), "packages.toml", &format!("[registries]\ndefault = \"{}\"\n", registry.display()));
    let Some((mut renderer, mut editor)) = world.open() else { return };
    open_window(&mut editor, &mut renderer);
    until(&mut editor, &mut renderer, "the packages being read", |editor| !editor.package_manager().is_busy());

    let add = find(&editor, "packages-add");
    click(&mut editor, &mut renderer, add);
    let item = menu_item(&editor.ui, "Add from Registry…").unwrap();
    click(&mut editor, &mut renderer, item);
    until(&mut editor, &mut renderer, "the registry's packages", |editor| !editor.package_manager().is_busy());
    frame(&mut editor, &mut renderer);
    let found = texts(&editor.ui, find(&editor, "packages-registry-list"));
    for text in ["orbit-tools", "1.0.0", "Cameras that orbit"] {
        assert!(found.iter().any(|shown| shown == text), "{text} in {found:?}");
    }
    shot(&mut renderer, "package-manager-registry.png");

    // Words no package has find nothing.
    let search = find(&editor, "packages-registry-search");
    editor.ui.focus(search);
    frame(&mut editor, &mut renderer);
    editor.ui.text_input("wheels");
    frame(&mut editor, &mut renderer);
    until(&mut editor, &mut renderer, "the search", |editor| !editor.package_manager().is_busy());
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.ui.text(find(&editor, "packages-registry-state")), "Nothing matches");
    for _ in 0.."wheels".len() {
        editor.ui.key_down(voxear_ui::Key::Backspace);
    }
    frame(&mut editor, &mut renderer);
    until(&mut editor, &mut renderer, "the search again", |editor| !editor.package_manager().is_busy());
    frame(&mut editor, &mut renderer);

    let add = find(&editor, "packages-registry-add");
    click(&mut editor, &mut renderer, add);
    until(&mut editor, &mut renderer, "the dialog", |editor| editor.package_manager().is_asking());
    frame(&mut editor, &mut renderer);
    let lines = texts(&editor.ui, find(&editor, "packages-dialog-lines"));
    assert!(lines.iter().any(|line| line.starts_with("Registry  ·  ")), "from the registry in {lines:?}");
    let add = find(&editor, "packages-dialog-add");
    click(&mut editor, &mut renderer, add);
    until(&mut editor, &mut renderer, "the package being listed", |editor| {
        let manager = editor.package_manager();
        !manager.rows().is_empty() && !manager.is_busy()
    });
    let written = std::fs::read_to_string(world.game().join("packages.toml")).unwrap();
    assert!(written.contains("orbit-tools = \"^1.0.0\""), "{written}");
}
