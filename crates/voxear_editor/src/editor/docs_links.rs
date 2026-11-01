//! Code pointing at VoxDocs, from the Inspector. `[Documentation]` puts a
//! book button on a behaviour's section or beside a field, which opens its
//! page in VoxDocs; `[Excerpt]` puts the part of a page it names above a
//! field, in a fold, drawn again whenever the pages change, so writing the
//! page in VoxDocs shows straight away. A link in an excerpt opens VoxDocs
//! at where it goes.

use voxear_ui::{NodeId, Ui};

use super::Editor;

impl Editor {
    pub(super) fn update_docs_links(&mut self) {
        if let Some(page) = self.inspector.clicked_documentation(&self.ui).map(str::to_string) {
            self.show_panel("docs");
            if !self.docs.open_reference(&mut self.ui, &page) {
                self.notify(&format!("There is no VoxDocs page called {page}"));
            }
        }
        let mut followed = None;
        let library = self.docs.library();
        let version = library.version();
        for fold in self.inspector.excerpts_mut() {
            if let Some(url) = self.ui.clicked_link(fold.fold)
                && let Some(excerpt) = library.excerpt(&fold.reference)
            {
                followed = Some((excerpt.page, url.to_string()));
            }
            if fold.shown == Some(version) {
                continue;
            }
            fold.shown = Some(version);
            let Some(content) = self.ui.foldout_content(fold.fold) else { continue };
            for child in self.ui.children(content) {
                self.ui.remove(child);
            }
            match library.excerpt(&fold.reference) {
                Some(excerpt) => {
                    set_title(&mut self.ui, fold.fold, &excerpt.title);
                    self.docs.render_excerpt(&mut self.ui, content, &excerpt);
                }
                None => {
                    let line = self.ui.add_label(content, &format!("There is no VoxDocs page called {}", fold.reference));
                    self.ui.add_class(line, "excerpt-missing");
                }
            }
        }
        drop(library);
        if let Some((page, url)) = followed {
            self.show_panel("docs");
            self.docs.follow_from(&mut self.ui, page, &url);
        }
    }
}

/// A foldout's title, which it is made with.
fn set_title(ui: &mut Ui, fold: NodeId, title: &str) {
    let Some(header) = ui.foldout_header(fold) else { return };
    if let Some(&label) = ui.children(header).get(2) {
        ui.set_text(label, title);
    }
}
