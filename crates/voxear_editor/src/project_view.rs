//! The Project panel: the project's assets as a tree of folders.

use voxear_assets::{AssetDb, AssetKind};
use voxear_ui::{NodeId, TreeItems, Ui};

struct Entry {
    name: String,
    /// `None` for a folder.
    kind: Option<AssetKind>,
    children: Vec<u64>,
}

struct Items {
    entries: Vec<Entry>,
    roots: Vec<u64>,
}

impl TreeItems for Items {
    fn children(&self, parent: Option<u64>) -> Vec<u64> {
        match parent {
            None => self.roots.clone(),
            Some(parent) => self.entries[parent as usize].children.clone(),
        }
    }

    fn has_children(&self, item: u64) -> bool {
        !self.entries[item as usize].children.is_empty()
    }

    fn make(&mut self, ui: &mut Ui, row: NodeId) {
        let icon = ui.add_label(row, "");
        ui.add_class(icon, "obj-icon");
        ui.add_label(row, "");
    }

    fn bind(&mut self, ui: &mut Ui, row: NodeId, item: u64) {
        let entry = &self.entries[item as usize];
        let icon = match entry.kind {
            None => "▤",
            Some(AssetKind::Level) => "◈",
            Some(AssetKind::Scene) => "◇",
            Some(AssetKind::Prefab) => "◆",
            Some(AssetKind::Model) => "▲",
            Some(AssetKind::Material) => "●",
            Some(AssetKind::Texture) => "▦",
        };
        let parts = ui.children(row);
        ui.set_text(parts[0], icon);
        ui.set_class(parts[0], "icon-folder", entry.kind.is_none());
        ui.set_class(parts[0], "icon-mesh", matches!(entry.kind, Some(AssetKind::Prefab | AssetKind::Model)));
        ui.set_text(parts[1], &entry.name);
    }
}

/// Fills `tree` with the assets in `db`, folders first and by name.
pub fn show(ui: &mut Ui, tree: NodeId, db: &AssetDb) {
    let mut paths: Vec<(&str, AssetKind)> = db.entries().map(|e| (e.path.as_str(), e.kind)).collect();
    paths.sort_by(|a, b| a.0.cmp(b.0));
    let mut items = Items { entries: Vec::new(), roots: Vec::new() };
    for (path, kind) in paths {
        let mut parent: Option<u64> = None;
        let parts: Vec<&str> = path.split('/').collect();
        for (depth, part) in parts.iter().enumerate() {
            let last = depth == parts.len() - 1;
            let siblings = match parent {
                None => &items.roots,
                Some(p) => &items.entries[p as usize].children,
            };
            let found = siblings.iter().copied().find(|&s| items.entries[s as usize].name == *part && !last);
            let id = match found {
                Some(id) => id,
                None => {
                    let id = items.entries.len() as u64;
                    let kind = if last { Some(kind) } else { None };
                    items.entries.push(Entry { name: part.to_string(), kind, children: Vec::new() });
                    match parent {
                        None => items.roots.push(id),
                        Some(p) => items.entries[p as usize].children.push(id),
                    }
                    id
                }
            };
            parent = Some(id);
        }
    }
    // Folders before files, as file managers list them.
    let folders_first =
        |entries: &[Entry], ids: &mut Vec<u64>| ids.sort_by_key(|&id| entries[id as usize].kind.is_some());
    let mut roots = std::mem::take(&mut items.roots);
    folders_first(&items.entries, &mut roots);
    items.roots = roots;
    for index in 0..items.entries.len() {
        let mut children = std::mem::take(&mut items.entries[index].children);
        folders_first(&items.entries, &mut children);
        items.entries[index].children = children;
    }
    ui.bind_tree(tree, items);
}
