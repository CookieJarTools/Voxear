//! The VoxDocs on a throwaway project: finding pages, following links,
//! ticking checklists and writing pages, without a GPU.

use std::path::Path;

use voxear_docs::{CodeReference, Library, Target, VoxDocs};
use voxear_ui::{NodeId, Style, Ui};

const WIDTH: u32 = 1000;
const HEIGHT: u32 = 700;

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let write = |path: &str, text: &str| {
        let file = dir.path().join("Docs").join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    };
    write("_folder.md", "---\nid: 0000root0000\n---\n\nThe team's own notes. Start at [[Writing Pages]].\n");
    write(
        "Guide/Writing Pages.md",
        "---\nid: a4f08c3e91b2\ntags: [docs, guide]\nupdated: 2026-09-19\n---\n\n\
         Pages are plain Markdown.\n\n## Checklists\n\n- [x] Write the page\n- [ ] Add a picture\n\n\
         See [damage](../Combat/Damage.md#hits), [the list](#checklists), [[Level Streaming]] and https://voxear.dev.\n",
    );
    write("Combat/Damage.md", "# Damage\n\n## Hits\n\nHow hits are counted.\n");
    std::fs::create_dir_all(dir.path().join("Models")).unwrap();
    std::fs::write(dir.path().join("Models/Crate.gltf"), "{}").unwrap();
    dir
}

#[test]
fn the_library_finds_pages_and_where_links_go() {
    let dir = project();
    let library = Library::open(dir.path(), "Hello");
    let root = library.root();
    assert_eq!(library.page(root).title(), "Hello", "the top folder is named after the project");
    let writing = library.find("Writing Pages").expect("by title");
    assert_eq!(library.find("a4f08c3e91b2"), Some(writing), "by id");
    assert_eq!(library.find("Guide/Writing Pages.md"), Some(writing), "by path");
    assert_eq!(library.page(writing).header.tags, ["docs", "guide"]);
    let titles: Vec<&str> = library.page(root).children.iter().map(|c| library.page(*c).title()).collect();
    assert_eq!(titles, ["Combat", "Guide"], "folders, by title");

    let damage = library.find("Combat/Damage").unwrap();
    assert_eq!(library.resolve("../Combat/Damage.md#hits", writing), Target::Page(damage, Some("hits".into())));
    assert_eq!(library.resolve("#checklists", writing), Target::Heading("checklists".into()));
    assert_eq!(library.resolve("docs:Level Streaming", writing), Target::Missing("Level Streaming".into()));
    assert_eq!(library.resolve("https://voxear.dev", writing), Target::Web("https://voxear.dev".into()));
    assert_eq!(library.resolve("Models/Crate.gltf", writing), Target::File("Models/Crate.gltf".into()));
    assert!(!library.works("docs:Level Streaming", writing));

    assert_eq!(library.search("hits counted"), [damage], "every word, anywhere in the page");
    assert_eq!(library.search("writing")[0], writing, "title matches first");
    assert!(library.search("nothing like this").is_empty());
}

#[test]
fn checklists_and_new_pages_are_saved() {
    let dir = project();
    let mut library = Library::open(dir.path(), "Hello");
    let writing = library.find("Writing Pages").unwrap();
    let blocks = voxear_docs::markdown::parse(&library.page(writing).body);
    let mut tasks = Vec::new();
    for block in &blocks {
        if let voxear_docs::markdown::Block::List { items, .. } = block {
            tasks.extend(items.iter().filter_map(|i| i.task));
        }
    }
    library.set_task(writing, tasks[1], true).unwrap();
    let file = std::fs::read_to_string(dir.path().join("Docs/Guide/Writing Pages.md")).unwrap();
    assert!(file.contains("- [x] Add a picture"), "ticked in the file:\n{file}");
    assert!(file.starts_with("---\nid: a4f08c3e91b2\n"), "the header stays");

    let guide = library.page(writing).parent.unwrap();
    let streaming = library.create(guide, "Level Streaming").unwrap();
    assert!(dir.path().join("Docs/Guide/Level Streaming.md").is_file());
    assert_eq!(library.find("Level Streaming"), Some(streaming));
    let writing = library.find("Writing Pages").unwrap();
    assert!(library.works("docs:Level Streaming", writing), "the link works now");
    assert_eq!(library.page(streaming).header.id.len(), 12, "a new page has an id");
}

/// The VoxDocs panel in a fixed box, a frame in.
fn panel(dir: &Path) -> (Ui, VoxDocs, NodeId) {
    let mut ui = Ui::new();
    let root = ui.root();
    let host = ui.panel(root, Style::column().at(0.0, 0.0).width(900.0).height(640.0));
    let docs = VoxDocs::new(&mut ui, host, dir, "Hello");
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert!(ui.take_warnings().is_empty(), "{:?}", ui.take_warnings());
    (ui, docs, host)
}

/// The first label anywhere in `node` whose text starts with `words`.
fn label(ui: &Ui, node: NodeId, words: &str) -> Option<NodeId> {
    if ui.text(node).starts_with(words) && ui.children(node).is_empty() {
        return Some(node);
    }
    ui.children(node).into_iter().find_map(|child| label(ui, child, words))
}

fn click_at(ui: &mut Ui, x: f32, y: f32) {
    ui.pointer_move(x, y);
    ui.pointer_button(true);
    ui.pointer_button(false);
}

#[test]
fn the_panel_follows_links_and_writes_missing_pages() {
    let dir = project();
    let (mut ui, mut docs, host) = panel(dir.path());
    let start = label(&ui, host, "The team's own notes").expect("the home page shows");
    let [x, y, w, h] = ui.rect(start);
    // "Writing Pages" ends the sentence, so it is at the right of the label's words.
    let words = ui.text(start);
    let at = words.find("Writing Pages").unwrap() as f32 + 6.0;
    let width = ui.rect(start)[2].min(words.len() as f32 * 6.6);
    click_at(&mut ui, x + width * at / words.len() as f32, y + h / 2.0);
    docs.update(&mut ui);
    ui.frame(WIDTH, HEIGHT, 1.0);
    let writing = docs.library().find("Writing Pages").unwrap();
    assert_eq!(docs.open_page(), writing, "the link opened the page ({x} {w})");
    assert!(label(&ui, host, "Pages are plain Markdown").is_some());

    let missing = label(&ui, host, "See damage").expect("the paragraph with links");
    let words = ui.text(missing);
    let [x, y, _, h] = ui.rect(missing);
    let width = ui.rect(missing)[2].min(words.len() as f32 * 6.6);
    let at = words.find("Level Streaming").unwrap() as f32 + 4.0;
    click_at(&mut ui, x + width * at / words.len() as f32, y + h / 2.0);
    docs.update(&mut ui);
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert!(dir.path().join("Docs/Guide/Level Streaming.md").is_file(), "a link to a missing page writes it");
    assert_eq!(docs.library().page(docs.open_page()).title(), "Level Streaming");
}

fn click(ui: &mut Ui, node: NodeId) {
    let [x, y, w, h] = ui.rect(node);
    click_at(ui, x + w / 2.0, y + h / 2.0);
}

/// An update and a frame, as the editor gives the panel each frame.
fn frame(ui: &mut Ui, docs: &mut VoxDocs) {
    docs.update(ui);
    ui.frame(WIDTH, HEIGHT, 1.0);
}

#[test]
fn edit_writes_the_title_tags_and_markdown_and_leaving_saves() {
    let dir = project();
    let (mut ui, mut docs, host) = panel(dir.path());
    assert!(docs.open_reference(&mut ui, "Writing Pages"));
    ui.frame(WIDTH, HEIGHT, 1.0);
    let named = |ui: &Ui, name: &str| ui.find(host, name).unwrap();
    let edit = named(&ui, "cb-edit");
    click(&mut ui, edit);
    frame(&mut ui, &mut docs);
    assert!(docs.is_editing());
    assert_eq!(ui.text(named(&ui, "cb-edit-label")), "Done");
    assert_eq!(ui.text(named(&ui, "cb-title-field")), "Writing Pages");
    let chips = |ui: &Ui| {
        let mut names = Vec::new();
        with_class(ui, ui.find(host, "cb-tag-picked").unwrap(), "cb-tag-name", &mut names);
        names.into_iter().map(|name| ui.text(name)).collect::<Vec<_>>()
    };
    assert_eq!(chips(&ui), ["docs", "guide"], "the page's tags, as chips");
    assert_eq!(ui.text(named(&ui, "cb-tags-field")), "");
    assert_eq!(ui.text(named(&ui, "cb-status")), "Saved");
    frame(&mut ui, &mut docs);
    let source = named(&ui, "cb-source");
    assert_eq!(ui.focused(), Some(source), "the Markdown takes the keyboard once it shows");
    assert!(docs.has_keyboard(&ui));

    ui.text_input("\nA new paragraph.");
    frame(&mut ui, &mut docs);
    assert_eq!(ui.text(named(&ui, "cb-status")), "Unsaved changes");
    assert!(ui.text(source).ends_with("https://voxear.dev.\n\nA new paragraph."), "typed at the end");
    std::thread::sleep(std::time::Duration::from_millis(300));
    frame(&mut ui, &mut docs);
    assert!(label(&ui, named(&ui, "cb-preview"), "A new paragraph").is_some(), "the preview follows");

    // A chip's × takes its tag off; a comma makes what is typed a tag; and a suggestion adds its own.
    let mut removes = Vec::new();
    with_class(&ui, host, "cb-tag-remove", &mut removes);
    click(&mut ui, removes[1]);
    frame(&mut ui, &mut docs);
    assert_eq!(chips(&ui), ["docs"]);
    ui.focus(named(&ui, "cb-tags-field"));
    ui.text_input("new, Docs, gu");
    frame(&mut ui, &mut docs);
    assert_eq!(chips(&ui), ["docs", "new"], "once each");
    assert_eq!(ui.text(named(&ui, "cb-tags-field")), "gu", "what is still being typed");
    let mut suggested = Vec::new();
    with_class(&ui, named(&ui, "cb-tag-suggestions"), "cb-tag-chip", &mut suggested);
    assert_eq!(suggested.len(), 1, "the project's tags that fit what is typed");
    click(&mut ui, suggested[0]);
    frame(&mut ui, &mut docs);
    assert_eq!(chips(&ui), ["docs", "new", "guide"]);
    ui.set_text(named(&ui, "cb-title-field"), "Writing Guide");
    // Set from code, so the field reports no change: what is typed would.
    ui.focus(named(&ui, "cb-title-field"));
    ui.text_input(" ");
    frame(&mut ui, &mut docs);
    docs.save(&mut ui);
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(ui.text(named(&ui, "cb-status")), "Saved");
    assert!(!dir.path().join("Docs/Guide/Writing Pages.md").exists(), "a new title renames the file");
    let saved = std::fs::read_to_string(dir.path().join("Docs/Guide/Writing Guide.md")).unwrap();
    assert!(saved.contains("tags: [docs, new, guide]"), "the chips: {saved}");
    assert!(saved.contains("id: a4f08c3e91b2"), "the page keeps its id");
    assert!(saved.ends_with("A new paragraph."));

    // Opening another page saves what was written.
    ui.focus(source);
    ui.text_input(" More.");
    frame(&mut ui, &mut docs);
    assert!(docs.open_reference(&mut ui, "Damage"));
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert!(!docs.is_editing());
    let saved = std::fs::read_to_string(dir.path().join("Docs/Guide/Writing Guide.md")).unwrap();
    assert!(saved.ends_with("A new paragraph. More."), "{saved}");
    let back = named(&ui, "cb-back");
    click(&mut ui, back);
    frame(&mut ui, &mut docs);
    assert_eq!(docs.library().page(docs.open_page()).title(), "Writing Guide", "Back finds the renamed page");
}

#[test]
fn pictures_show_once_the_editor_has_read_them() {
    let dir = project();
    let page =
        "Pictures beside the page:\n\n![A shot](images/shot.png)\n\n![](images/wide.png)\n\n![Gone](images/gone.png)\n";
    std::fs::write(dir.path().join("Docs/Combat/Pictures.md"), page).unwrap();
    std::fs::create_dir_all(dir.path().join("Docs/Combat/images")).unwrap();
    for name in ["shot.png", "wide.png"] {
        std::fs::write(dir.path().join("Docs/Combat/images").join(name), "not read by the panel").unwrap();
    }
    let (mut ui, mut docs, host) = panel(dir.path());
    assert!(docs.open_reference(&mut ui, "Pictures"));
    ui.frame(WIDTH, HEIGHT, 1.0);
    let mut placeholders = Vec::new();
    with_class(&ui, host, "md-image", &mut placeholders);
    assert_eq!(placeholders.len(), 3, "each shows its name until it is read");
    let loading: Vec<bool> = placeholders.iter().map(|&p| ui.has_class(p, "md-loading")).collect();
    assert_eq!(loading, [true, true, false], "the one that is not there is not waited for");

    let wanted = docs.wanted_pictures(8);
    let images = dir.path().join("Docs/Combat/images");
    assert_eq!(wanted, [images.join("shot.png"), images.join("wide.png")]);
    assert!(docs.wanted_pictures(8).is_empty(), "each is asked for once");
    let image = |texture| voxear_ui::ImageRef { texture, uv: [0.0, 0.0, 1.0, 1.0] };
    docs.set_picture(&mut ui, &images.join("shot.png"), Some((image(3), [200, 100])));
    docs.set_picture(&mut ui, &images.join("wide.png"), Some((image(4), [4000, 1000])));
    frame(&mut ui, &mut docs);
    let mut pictures = Vec::new();
    with_class(&ui, host, "md-picture", &mut pictures);
    assert_eq!(pictures.len(), 2);
    let [_, _, width, height] = ui.rect(pictures[0]);
    assert_eq!((width, height), (200.0, 100.0), "at its own size");
    let [_, _, width, height] = ui.rect(pictures[1]);
    let [_, _, page_width, _] = ui.rect(ui.find(host, "cb-page").unwrap());
    assert!(width < page_width, "narrower to fit the page");
    assert!((width / height - 4.0).abs() < 0.05, "keeping its shape: {width} by {height}");

    // A next frame at the same size, as a GIF's, is swapped in where it shows; the page is not drawn again.
    docs.set_picture(&mut ui, &images.join("shot.png"), Some((image(5), [200, 100])));
    frame(&mut ui, &mut docs);
    let mut again = Vec::new();
    with_class(&ui, host, "md-picture", &mut again);
    assert_eq!(again, pictures, "the same pictures, not drawn again");
}

/// Every element in `node` with `class`, in tree order.
fn with_class(ui: &Ui, node: NodeId, class: &str, found: &mut Vec<NodeId>) {
    if ui.has_class(node, class) {
        found.push(node);
    }
    for child in ui.children(node) {
        with_class(ui, child, class, found);
    }
}

#[test]
fn tags_filter_the_pages_and_a_long_page_lists_its_headings() {
    let dir = project();
    let section = "Each hit takes some of what is left.\n\n".repeat(12);
    let long = format!(
        "---\ntags: [combat, guide]\n---\n\n## Hits\n\n{section}## Armour\n\n{section}### Plates\n\n{section}\
         ## Healing\n\n{section}"
    );
    std::fs::write(dir.path().join("Docs/Combat/Damage.md"), long).unwrap();
    let (mut ui, mut docs, host) = panel(dir.path());
    let named = |ui: &Ui, name: &str| ui.find(host, name).unwrap();
    let chips = |ui: &Ui| {
        let mut chips = Vec::new();
        with_class(ui, host, "cb-tag-chip", &mut chips);
        chips.iter().map(|&chip| texts(ui, chip).join(" ")).collect::<Vec<_>>()
    };
    assert_eq!(chips(&ui), ["guide 2", "combat 1", "docs 1"], "the most used first, with their counts");

    // Picking a tag shows only the pages with it; a second narrows them further.
    let mut found = Vec::new();
    with_class(&ui, host, "cb-tag-chip", &mut found);
    click(&mut ui, found[0]);
    frame(&mut ui, &mut docs);
    let results = named(&ui, "cb-results");
    assert!(ui.is_visible(results) && !ui.is_visible(named(&ui, "cb-tree")));
    let titles = |docs: &VoxDocs| -> Vec<String> {
        let library = docs.library();
        let mut titles: Vec<String> =
            library.search_tagged("", &["guide".into()]).iter().map(|&p| library.page(p).title().to_string()).collect();
        titles.sort();
        titles
    };
    assert_eq!(titles(&docs), ["Damage", "Writing Pages"]);
    assert_eq!(docs.library().search_tagged("", &["guide".into(), "COMBAT".into()]).len(), 1, "every tag, any case");
    assert!(ui.is_visible(named(&ui, "cb-tags-clear")));
    let clear = named(&ui, "cb-tags-clear");
    click(&mut ui, clear);
    frame(&mut ui, &mut docs);
    assert!(ui.is_visible(named(&ui, "cb-tree")), "Clear goes back to the tree");

    // A long page lists its headings beside it, and a click on one goes there.
    assert!(docs.open_reference(&mut ui, "Damage"));
    ui.frame(WIDTH, HEIGHT, 1.0);
    frame(&mut ui, &mut docs);
    let toc = named(&ui, "cb-toc");
    assert_eq!(texts(&ui, toc), ["On this page", "Hits", "Armour", "Plates", "Healing"]);
    assert!(ui.is_visible(toc), "the page is wide enough for it");
    let mut links = Vec::new();
    with_class(&ui, toc, "cb-toc-link", &mut links);
    assert!(ui.has_class(links[0], "current"), "the first heading is the one being read");
    let page = named(&ui, "cb-page");
    ui.set_scroll(page, [0.0, 10_000.0]);
    frame(&mut ui, &mut docs);
    let current: Vec<bool> = links.iter().map(|&link| ui.has_class(link, "current")).collect();
    assert_eq!(current, [false, false, false, true], "scrolled to the end, the last");
    let armour = links[1];
    click(&mut ui, armour);
    frame(&mut ui, &mut docs);
    frame(&mut ui, &mut docs);
    assert!(ui.has_class(armour, "current"), "a click goes to its heading");

    // A tag on the page picks it.
    ui.set_scroll(page, [0.0, 0.0]);
    ui.frame(WIDTH, HEIGHT, 1.0);
    let mut on_page = Vec::new();
    with_class(&ui, named(&ui, "cb-page"), "cb-tag", &mut on_page);
    click(&mut ui, on_page[0]);
    frame(&mut ui, &mut docs);
    assert!(ui.is_visible(named(&ui, "cb-results")), "the page's tag filters the pages");
}

/// The words in `node` and everything in it, in tree order.
fn texts(ui: &Ui, node: NodeId) -> Vec<String> {
    let mut out = vec![ui.text(node)];
    for child in ui.children(node) {
        out.extend(texts(ui, child));
    }
    out.retain(|text| !text.is_empty());
    out
}

#[test]
fn health_lists_what_needs_looking_after() {
    let dir = project();
    std::fs::create_dir_all(dir.path().join("Levels")).unwrap();
    std::fs::write(dir.path().join("Levels/Arena.level"), "").unwrap();
    let damage = "---\nid: 7b1c0d2e3f40\nupdated: 2020-01-01\n---\n\n\
                  See [the arena](Levels/Arena.level) and [[Armour]].\n\n![](images/gone.png)\n";
    std::fs::write(dir.path().join("Docs/Combat/Damage.md"), damage).unwrap();
    std::fs::create_dir_all(dir.path().join("Docs/Combat/images")).unwrap();
    std::fs::write(dir.path().join("Docs/Combat/images/old.png"), "not read").unwrap();

    let library = Library::open(dir.path(), "Hello");
    let health = voxear_docs::health::Health::check(&library);
    let title = |id| library.page(id).title().to_string();
    let stale: Vec<(String, Vec<String>)> =
        health.stale.iter().map(|(id, files)| (title(*id), files.clone())).collect();
    assert_eq!(stale, [("Damage".to_string(), vec!["Levels/Arena.level".to_string()])], "the level changed after 2020");
    let mut broken: Vec<(String, String)> = health.broken.iter().map(|(id, url)| (title(*id), url.clone())).collect();
    broken.sort();
    assert_eq!(
        broken,
        [("Damage".into(), "docs:Armour".into()), ("Writing Pages".into(), "docs:Level Streaming".into())]
    );
    let missing: Vec<(String, String)> =
        health.missing_pictures.iter().map(|(id, url)| (title(*id), url.clone())).collect();
    assert_eq!(missing, [("Damage".to_string(), "images/gone.png".to_string())]);
    assert_eq!(health.unused_pictures, ["Docs/Combat/images/old.png"]);
    assert_eq!(health.problems(), 5);

    // The panel shows it in the page's place, and Health again goes back.
    let (mut ui, mut docs, host) = panel(dir.path());
    let button = ui.find(host, "cb-health").unwrap();
    click(&mut ui, button);
    frame(&mut ui, &mut docs);
    let page = ui.find(host, "cb-page").unwrap();
    let shown = texts(&ui, page);
    for line in
        ["Docs Health", "5 things need looking after.", "Out of date", "Links that go nowhere", "Missing pictures"]
    {
        assert!(shown.iter().any(|text| text == line), "{line} in {shown:?}");
    }
    assert!(shown.iter().any(|text| text == "Pictures no page shows"));
    click(&mut ui, button);
    frame(&mut ui, &mut docs);
    assert!(label(&ui, host, "The team's own notes").is_some(), "back to the page");
}

#[test]
fn home_lists_recent_pages_and_how_healthy_the_docs_are() {
    let dir = project();
    let (mut ui, mut docs, host) = panel(dir.path());
    let page = ui.find(host, "cb-page").unwrap();
    let shown = texts(&ui, page);
    let recent = shown.iter().position(|text| text == "Recently updated").expect("home lists recent pages");
    assert_eq!(shown[recent + 1..recent + 3], ["Writing Pages", "2026-09-19"], "the one page with a date");
    let health = label(&ui, page, "One thing needs looking after").expect("the missing page Writing Pages links to");
    click(&mut ui, health);
    frame(&mut ui, &mut docs);
    assert!(label(&ui, page, "Docs Health").is_some(), "the line opens Docs Health");
}

/// A Code Reference as the scripts tool writes it: a type of the game's,
/// and two of the engine's, one named as a page of the project's.
const CODE: &str = r#"{ "format": 1, "types": [
    { "key": "Hello.Spinner", "namespace": "Hello", "name": "Spinner", "kind": "class",
      "declaration": "public sealed class Spinner : Behaviour", "assembly": "Hello.Scripts",
      "summary": "Spins its Obj, as a [[code:Voxear.Prefab|Prefab]] might.", "remarks": "",
      "documentation": "Combat/Damage",
      "members": [{ "kind": "field", "name": "degreesPerSecond", "summary": "How fast it turns.",
                    "declaration": "[SerializeField] private float degreesPerSecond",
                    "documentation": "Combat/Damage#hits" }] },
    { "key": "Voxear.Prefab", "namespace": "Voxear", "name": "Prefab", "kind": "struct", "folder": "Content/Loading",
      "declaration": "public struct Prefab", "assembly": "Voxear.Core", "summary": "A loaded prefab.", "members": [] },
    { "key": "Voxear.Damage", "namespace": "Voxear", "name": "Damage", "kind": "struct",
      "declaration": "public struct Damage", "assembly": "Voxear.Core", "summary": "", "members": [],
      "documentation": "Combat/Armour" }
] }"#;

#[test]
fn the_code_reference_is_pages_of_its_own_after_the_projects() {
    let dir = project();
    let mut library = Library::open(dir.path(), "Hello");
    let damage = library.find("Damage").unwrap();
    library.set_code(CodeReference::parse(CODE).unwrap());
    let root = library.root();
    let titles: Vec<&str> = library.page(root).children.iter().map(|c| library.page(*c).title()).collect();
    assert_eq!(titles, ["Combat", "Guide", "Code Reference"]);
    let code = library.find("code:").unwrap();
    let namespaces: Vec<&str> = library.page(code).children.iter().map(|c| library.page(*c).title()).collect();
    assert_eq!(namespaces, ["Hello", "Voxear"], "the game's first");

    let prefab = library.find("code:Voxear.Prefab").unwrap();
    let mut trail = Vec::new();
    let mut at = library.page(prefab).parent;
    while let Some(folder) = at.filter(|&f| f != code) {
        trail.insert(0, library.page(folder).title());
        at = library.page(folder).parent;
    }
    assert_eq!(trail, ["Voxear", "Content", "Loading"], "in the folders its source is in");
    let voxear: Vec<&str> =
        library.page(library.find("code:Voxear").unwrap()).children.iter().map(|c| library.page(*c).title()).collect();
    assert_eq!(voxear, ["Content", "Damage"], "folders first");

    let spinner = library.find("code:Hello.Spinner").expect("a type's page, by its full name");
    assert!(library.page(spinner).generated);
    assert!(library.page(spinner).body.contains("How fast it turns."));
    let prefab = library.find("code:Voxear.Prefab").unwrap();
    assert_eq!(library.resolve("docs:code:Voxear.Prefab", spinner), Target::Page(prefab, None));
    assert_eq!(library.resolve("docs:code:Voxear.Gone", spinner), Target::Missing("code:Voxear.Gone".into()));
    assert_eq!(library.find("Damage"), Some(damage), "a page named as a type is still found by its title");
    assert_eq!(library.search("damage")[0], damage, "and comes before it in a search");
    assert!(library.save(code, "Changed").is_err(), "a folder made from the code is not written");

    // Made again, as after the scripts build, the project's pages keep their places.
    library.set_code(CodeReference::default());
    assert_eq!(library.find("code:Hello.Spinner"), None);
    assert_eq!(library.find("Damage"), Some(damage));
    let empty = tempfile::tempdir().unwrap();
    let mut library = Library::open(empty.path(), "Empty");
    library.set_code(CodeReference::parse(CODE).unwrap());
    assert!(library.is_empty(), "the Code Reference alone is no pages of the project's");
}

#[test]
fn code_pages_take_notes_and_their_folders_are_read_only() {
    let dir = project();
    let (mut ui, mut docs, host) = panel(dir.path());
    docs.set_code(&mut ui, CodeReference::parse(CODE).unwrap());
    assert!(docs.open_reference(&mut ui, "code:Hello"));
    frame(&mut ui, &mut docs);
    let edit = ui.find(host, "cb-edit").unwrap();
    assert!(!ui.is_visible(edit), "a namespace's folder has no Edit");
    assert!(docs.open_reference(&mut ui, "code:Hello.Spinner"));
    frame(&mut ui, &mut docs);
    assert!(label(&ui, host, "How fast it turns").is_some(), "the page shows its members");
    assert!(ui.is_visible(edit), "a type's page has Edit, for its note");

    // The scripts built again: the page stays open, and Edit comes back with the project's own pages.
    docs.set_code(&mut ui, CodeReference::parse(CODE).unwrap());
    assert_eq!(docs.library().page(docs.open_page()).title(), "Spinner");
    assert!(docs.open_reference(&mut ui, "Writing Pages"));
    frame(&mut ui, &mut docs);
    assert!(ui.is_visible(edit));
    assert!(ui.take_warnings().is_empty(), "{:?}", ui.take_warnings());
}

#[test]
fn code_points_at_pages_and_excerpts_show_their_sections() {
    let dir = project();
    let mut library = Library::open(dir.path(), "Hello");
    library.set_code(CodeReference::parse(CODE).unwrap());
    let damage = library.find("Combat/Damage").unwrap();
    assert_eq!(
        library.code_links(damage),
        [
            ("Spinner".to_string(), "docs:code:Hello.Spinner".to_string()),
            ("Spinner.degreesPerSecond".to_string(), "docs:code:Hello.Spinner#degreespersecond".to_string()),
        ],
        "In the code, by type and member"
    );
    let spinner = library.find("code:Hello.Spinner").unwrap();
    let body = &library.page(spinner).body;
    assert!(body.contains("Documented in [[Combat/Damage|Damage]]."), "{body}");
    assert!(body.contains("Documented in [[Combat/Damage#hits|Damage]]."), "a member's too: {body}");

    // [Documentation] naming a page that is not there shows in Docs Health.
    let health = voxear_docs::health::Health::check(&library);
    let armour = library.find("code:Voxear.Damage").unwrap();
    assert!(health.broken.contains(&(armour, "docs:Combat/Armour".to_string())), "{:?}", health.broken);

    let hits = library.excerpt("Combat/Damage#hits").expect("a section");
    assert_eq!((hits.page, hits.title.as_str()), (damage, "Hits"));
    assert_eq!(voxear_docs::markdown::plain_text(&hits.blocks), "How hits are counted.\n");
    assert_eq!(library.excerpt("Combat/Damage").unwrap().title, "Damage", "the whole page");
    assert!(library.excerpt("Combat/Damage#nowhere").is_none() && library.excerpt("Combat/Armour").is_none());

    let before = library.version();
    library.save(damage, "## Hits\n\nCounted twice.\n").unwrap();
    assert!(library.version() > before, "saving a page tells what shows it");
    let hits = library.excerpt("Combat/Damage#hits").unwrap();
    assert_eq!(voxear_docs::markdown::plain_text(&hits.blocks), "Counted twice.\n");
}

#[test]
fn a_guide_runs_its_actions_watches_its_checks_and_remembers() {
    let dir = project();
    let guide = "# Set up\n\n1. Light it.\n\n   [Add a light](<action:New Point Light>)\n\n[It is lit](check:obj/Sun)\n\n\
                 [The lamp is there](check:file/Prefabs/Lamp.prefab)\n";
    std::fs::write(dir.path().join("Docs/Guide/Set Up.md"), guide).unwrap();
    let (mut ui, mut docs, host) = panel(dir.path());
    assert!(docs.open_reference(&mut ui, "Set Up"));
    frame(&mut ui, &mut docs);
    let line = |ui: &Ui| {
        let mut lines = Vec::new();
        with_class(ui, host, "cb-progress-line", &mut lines);
        lines.first().copied()
    };
    assert_eq!(ui.text(line(&ui).expect("how far the guide has got")), "0 of 3 steps done");
    assert_eq!(docs.wanted_checks(), ["obj/Sun", "file/Prefabs/Lamp.prefab"]);

    let mut buttons = Vec::new();
    with_class(&ui, host, "md-step-action", &mut buttons);
    click(&mut ui, buttons[0]);
    frame(&mut ui, &mut docs);
    assert_eq!(docs.take_action_request().as_deref(), Some("New Point Light"), "the command, by its title");
    assert_eq!(docs.take_action_request(), None, "once");
    docs.action_ran(&mut ui);
    docs.set_check(&mut ui, "obj/Sun", Ok(()));
    docs.set_check(&mut ui, "file/Prefabs/Lamp.prefab", Err("There is no Prefabs/Lamp.prefab yet".into()));
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(ui.text(line(&ui).unwrap()), "2 of 3 steps done");
    let mut done = Vec::new();
    with_class(&ui, host, "md-step-done", &mut done);
    assert_eq!(done.len(), 2);
    assert!(label(&ui, host, "There is no Prefabs/Lamp.prefab yet").is_some_and(|note| ui.is_visible(note)));

    // Another session remembers the action; Reset starts the guide again.
    let (mut ui, mut docs, host) = panel(dir.path());
    let line = |ui: &Ui| {
        let mut lines = Vec::new();
        with_class(ui, host, "cb-progress-line", &mut lines);
        lines.first().copied()
    };
    docs.open_reference(&mut ui, "Set Up");
    frame(&mut ui, &mut docs);
    assert_eq!(ui.text(line(&ui).unwrap()), "1 of 3 steps done", "the action is kept, checks look again");
    let reset = label(&ui, host, "Reset").unwrap();
    click(&mut ui, reset);
    frame(&mut ui, &mut docs);
    assert_eq!(ui.text(line(&ui).unwrap()), "0 of 3 steps done");
}

#[test]
fn notes_on_code_pages_last_and_health_finds_lost_ones_and_code_with_no_page() {
    let dir = project();
    let mut library = Library::open(dir.path(), "Hello");
    library.set_code(CodeReference::parse(CODE).unwrap());
    let spinner = library.find("code:Hello.Spinner").unwrap();
    library.save_page(spinner, "Ignored", &["combat".to_string()], "Turns **twice** as fast at night.").unwrap();
    let file = dir.path().join("Docs/.code-notes/Hello.Spinner.md");
    let text = std::fs::read_to_string(&file).expect("the note is kept in .code-notes");
    assert!(text.contains("tags: [combat]") && text.ends_with("Turns **twice** as fast at night."), "{text}");
    let spinner = library.find("code:Hello.Spinner").unwrap();
    assert_eq!(library.page(spinner).title(), "Spinner", "the page keeps its type's name");
    assert_eq!(library.page(spinner).header.tags, ["combat"]);
    assert!(library.page(spinner).body.contains("## Notes\n\nTurns **twice** as fast at night."));
    assert_eq!(library.notes_of(spinner), "Turns **twice** as fast at night.");
    let titles: Vec<&str> = library.page(library.root()).children.iter().map(|c| library.page(*c).title()).collect();
    assert_eq!(titles, ["Combat", "Guide", "Code Reference"], "the notes' folder is not a page");

    // The type renamed, its note is lost; and a type of the project's with no [Documentation] has no page yet.
    let renamed =
        CODE.replace("Hello.Spinner", "Hello.Turner").replace("\"name\": \"Spinner\"", "\"name\": \"Turner\"");
    let renamed = renamed.replace("\"documentation\": \"Combat/Damage\",", "");
    library.set_code(CodeReference::parse(&renamed).unwrap());
    let health = voxear_docs::health::Health::check(&library);
    assert_eq!(health.lost_notes, ["Hello.Spinner"]);
    let turner = library.find("code:Hello.Turner").unwrap();
    assert_eq!(health.undocumented, [turner], "the engine's own types are not the project's to document");

    // A note emptied goes, file and all.
    library.set_code(CodeReference::parse(CODE).unwrap());
    let spinner = library.find("code:Hello.Spinner").unwrap();
    library.save_page(spinner, "", &[], "  ").unwrap();
    assert!(!file.exists());
}

#[derive(Clone, Default)]
struct SharedClipboard(std::rc::Rc<std::cell::RefCell<Option<String>>>);

impl voxear_ui::Clipboard for SharedClipboard {
    fn read(&mut self) -> Option<String> {
        self.0.borrow().clone()
    }

    fn write(&mut self, text: &str) {
        *self.0.borrow_mut() = Some(text.to_string());
    }
}

#[test]
fn a_pages_menu_copies_its_documentation_attribute() {
    let dir = project();
    let (mut ui, mut docs, host) = panel(dir.path());
    let clipboard = SharedClipboard::default();
    ui.set_clipboard(Box::new(clipboard.clone()));
    assert!(docs.open_reference(&mut ui, "Writing Pages"));
    frame(&mut ui, &mut docs);
    frame(&mut ui, &mut docs);
    let mut titles = Vec::new();
    with_class(&ui, host, "cb-row-title", &mut titles);
    let row = titles.into_iter().find(|&t| ui.text(t) == "Writing Pages").expect("the page in the tree");
    let [x, y, w, h] = ui.rect(row);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    ui.pointer_secondary(true);
    ui.pointer_secondary(false);
    frame(&mut ui, &mut docs);
    let mut items = Vec::new();
    with_class(&ui, ui.root(), "menu-item", &mut items);
    let named = |ui: &Ui, item: NodeId, words: &str| ui.children(item).iter().any(|&part| ui.text(part) == words);
    let attribute = items.into_iter().find(|&i| named(&ui, i, "Copy [Documentation] attribute")).expect("offered");
    click(&mut ui, attribute);
    frame(&mut ui, &mut docs);
    assert_eq!(clipboard.0.borrow().as_deref(), Some("[Documentation(\"a4f08c3e91b2\")]"), "named by its id");
}

/// Runs git in `dir`, as a commit made on `date` would.
fn git(dir: &Path, date: &str, args: &[&str]) {
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com", "-c", "commit.gpgsign=false"])
        .args(args)
        .env("GIT_AUTHOR_DATE", date)
        .env("GIT_COMMITTER_DATE", date)
        .status()
        .expect("git runs");
    assert!(status.success(), "git {args:?}");
}

#[test]
fn a_page_is_out_of_date_by_gits_days_not_its_header() {
    let dir = project();
    std::fs::create_dir_all(dir.path().join("Levels")).unwrap();
    std::fs::write(dir.path().join("Levels/Arena.level"), "format = 1\n").unwrap();
    // The header says the page is newer than the level, but the page last changed in January.
    let page = "---\nid: 7b1c0d2e3f40\nupdated: 2026-09-01\n---\n\nSee [the arena](Levels/Arena.level).\n";
    std::fs::write(dir.path().join("Docs/Combat/Damage.md"), page).unwrap();
    git(dir.path(), "2026-01-01T12:00:00Z", &["init", "-q"]);
    git(dir.path(), "2026-01-01T12:00:00Z", &["add", "Docs"]);
    git(dir.path(), "2026-01-01T12:00:00Z", &["commit", "-q", "-m", "Pages"]);
    git(dir.path(), "2026-02-01T12:00:00Z", &["add", "Levels"]);
    git(dir.path(), "2026-02-01T12:00:00Z", &["commit", "-q", "-m", "The arena"]);

    let library = Library::open(dir.path(), "Hello");
    let damage = library.find("Combat/Damage").unwrap();
    let health = voxear_docs::health::Health::check(&library);
    assert_eq!(health.stale, [(damage, vec!["Levels/Arena.level".to_string()])], "the level changed after the page");

    // The page written again, committed later: no longer out of date, once Health looks again.
    std::fs::write(dir.path().join("Docs/Combat/Damage.md"), page.replace("See", "Now see")).unwrap();
    git(dir.path(), "2026-03-01T12:00:00Z", &["commit", "-q", "-am", "Damage again"]);
    library.forget_history();
    assert!(voxear_docs::health::Health::check(&library).stale.is_empty());
}

#[test]
fn health_counts_the_comments_the_projects_types_still_need() {
    let dir = project();
    let mut library = Library::open(dir.path(), "Hello");
    library.set_code(CodeReference::parse(&CODE.replace("How fast it turns.", "")).unwrap());
    let spinner = library.find("code:Hello.Spinner").unwrap();
    let health = voxear_docs::health::Health::check(&library);
    assert_eq!(health.comment_gaps, [(spinner, 1, 2)], "the field has none; the engine's types are not counted");
    assert_eq!(health.problems(), 2, "nor are these among the things to look after");
}

#[test]
fn files_dropped_while_writing_go_into_the_page() {
    let dir = project();
    let outside = tempfile::tempdir().unwrap();
    let shot = outside.path().join("Arena Shot.png");
    std::fs::write(&shot, "a picture").unwrap();
    let (mut ui, mut docs, host) = panel(dir.path());
    assert!(docs.open_reference(&mut ui, "Combat/Damage"));
    frame(&mut ui, &mut docs);
    assert!(!docs.drop_file(&mut ui, &shot), "not while reading");

    let edit = ui.find(host, "cb-edit").unwrap();
    click(&mut ui, edit);
    frame(&mut ui, &mut docs);
    frame(&mut ui, &mut docs);
    assert!(docs.drop_file(&mut ui, &shot));
    assert!(docs.drop_file(&mut ui, &dir.path().join("Models/Crate.gltf")));
    frame(&mut ui, &mut docs);
    let kept = dir.path().join("Docs/Combat/images/Arena Shot.png");
    assert_eq!(std::fs::read_to_string(&kept).unwrap(), "a picture", "copied beside the page");
    let source = ui.find(host, "cb-source").unwrap();
    let text = ui.text(source);
    assert!(text.contains("![Arena Shot](<images/Arena Shot.png>)\n"), "{text}");
    assert!(text.contains("![Crate](Models/Crate.gltf)\n"), "a model in the project, shown live: {text}");
}

/// Clicks the open menu's item that says `label`.
fn choose(ui: &mut Ui, docs: &mut VoxDocs, label: &str) {
    let mut items = Vec::new();
    with_class(ui, ui.root(), "menu-item", &mut items);
    let item = items
        .into_iter()
        .find(|&item| ui.children(item).iter().any(|&part| ui.text(part) == label))
        .unwrap_or_else(|| panic!("no menu item says {label}"));
    click(ui, item);
    frame(ui, docs);
}

#[test]
fn image_link_and_action_put_markdown_in_while_writing() {
    let dir = project();
    let (mut ui, mut docs, host) = panel(dir.path());
    assert!(docs.open_reference(&mut ui, "Writing Pages"));
    frame(&mut ui, &mut docs);
    let edit = ui.find(host, "cb-edit").unwrap();
    click(&mut ui, edit);
    frame(&mut ui, &mut docs);
    frame(&mut ui, &mut docs);
    let source = ui.find(host, "cb-source").unwrap();

    let image = ui.find(host, "cb-insert-image").unwrap();
    click(&mut ui, image);
    frame(&mut ui, &mut docs);
    choose(&mut ui, &mut docs, "Models/Crate.gltf");
    assert!(ui.text(source).contains("![Crate](Models/Crate.gltf)"), "{}", ui.text(source));

    let link = ui.find(host, "cb-insert-link").unwrap();
    click(&mut ui, link);
    frame(&mut ui, &mut docs);
    choose(&mut ui, &mut docs, "Combat/Damage");
    assert!(ui.text(source).contains("[[Damage]]"), "{}", ui.text(source));

    // Action… asks the editor for its commands, then lists them with the checks.
    let action = ui.find(host, "cb-insert-action").unwrap();
    click(&mut ui, action);
    frame(&mut ui, &mut docs);
    assert!(docs.take_commands_wanted());
    docs.open_action_menu(&mut ui, &["Play".to_string(), "New Point Light".to_string()], &[]);
    frame(&mut ui, &mut docs);
    choose(&mut ui, &mut docs, "New Point Light");
    assert!(ui.text(source).contains("[New Point Light](<action:New Point Light>)"), "{}", ui.text(source));
}

#[test]
fn a_box_finds_a_tag_among_many() {
    let dir = project();
    let tags = ["combat", "camera", "lighting", "levels", "audio", "input", "ui", "saving", "net", "combo"];
    for (index, tag) in tags.iter().enumerate() {
        std::fs::write(
            dir.path().join(format!("Docs/Page {index}.md")),
            format!("---\ntags: [{tag}]\n---\n\nWords.\n"),
        )
        .unwrap();
    }
    let (mut ui, mut docs, host) = panel(dir.path());
    let find = ui.find(host, "cb-tag-find").unwrap();
    assert!(ui.is_visible(find), "shown once there are more tags than show at first");
    ui.focus(find);
    ui.text_input("com");
    frame(&mut ui, &mut docs);
    let mut names = Vec::new();
    with_class(&ui, ui.find(host, "cb-tag-chips").unwrap(), "cb-tag-name", &mut names);
    let mut shown: Vec<String> = names.into_iter().map(|name| ui.text(name)).collect();
    shown.sort();
    assert_eq!(shown, ["combat", "combo"], "the tags with the words in them");
    assert!(!ui.is_visible(ui.find(host, "cb-tags-more").unwrap()), "every one that fits shows");
}
