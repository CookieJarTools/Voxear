//! The VoxDocs on a throwaway project: finding pages, following links,
//! ticking checklists and writing pages, without a GPU.

use std::path::Path;

use voxear_docs::{Library, Target, VoxDocs};
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
    assert_eq!(ui.text(named(&ui, "cb-tags-field")), "docs, guide");
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

    ui.set_text(named(&ui, "cb-title-field"), "Writing Guide");
    ui.set_text(named(&ui, "cb-tags-field"), "docs, Guide, new, docs");
    // Set from code, so the field reports no change: what is typed would.
    ui.focus(named(&ui, "cb-title-field"));
    ui.text_input(" ");
    frame(&mut ui, &mut docs);
    docs.save(&mut ui);
    ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(ui.text(named(&ui, "cb-status")), "Saved");
    assert!(!dir.path().join("Docs/Guide/Writing Pages.md").exists(), "a new title renames the file");
    let saved = std::fs::read_to_string(dir.path().join("Docs/Guide/Writing Guide.md")).unwrap();
    assert!(saved.contains("tags: [docs, Guide, new]"), "tags kept once each: {saved}");
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
