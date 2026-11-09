//! Data assets in the Inspector (see `crate::data_assets`): a `.asset`
//! selected in the Project panel shows the fields of the Definition it
//! holds, on an Obj made for it alone, and each change is written into the
//! file at once, as a Gun Handling's is, the open level left as it was, and
//! into the instance the scripts loaded, so a game playing sees it. The Obj
//! goes once no Inspector shows it. The Create menu's New Data Asset…
//! offers each Definition the scripts have, in the folders its
//! `[CreateAsset]` path names, and makes a file of the one picked.

use voxear_ecs::Entity;
use voxear_render::Renderer;
use voxear_ui::NodeId;

use super::Editor;
use crate::{
    data_assets::{self, DataAssetHolder},
    inspector::{menu_place, shared_namespace},
    object_picker::{Choice, Chosen, PickFor},
    project_view::{ProjectEvent, ProjectItem},
};

impl Editor {
    /// Shows the data asset `item` in the Inspector, its fields on an Obj of
    /// their own. False when its file cannot be read, to show it as any
    /// other file instead.
    pub(super) fn inspect_data(&mut self, item: &ProjectItem) -> bool {
        let file = self.game.content.db().file(&item.path);
        let data = match data_assets::read(&file) {
            Ok(data) => data,
            Err(error) => {
                tracing::warn!("{error:#}");
                return false;
            }
        };
        let holder = data_assets::spawn_holder(&mut self.game.engine.world, &item.path, data);
        let playing = self.is_playing();
        self.inspector.show_data(&mut self.ui, &self.game, item, holder, playing);
        true
    }

    /// Whether `entity` is the Obj a data asset shows on, whose changes go
    /// into the asset's file rather than the level, with nothing to undo.
    pub(super) fn holds_data(&self, entity: Entity) -> bool {
        self.game.engine.world.has::<DataAssetHolder>(entity)
    }

    /// Writes the fields of each data asset shown that the Inspector changed
    /// into its file, and takes away the Objs of those no Inspector shows
    /// any more, in the edited level put aside while the game plays too. A
    /// data asset whose file went, deleted or moved away, goes from the
    /// Inspector rather than being written again.
    pub(super) fn update_data_assets(&mut self) {
        let shown = self.data_shown();
        let (world, db) = (&self.game.engine.world, self.game.content.db());
        let holders: Vec<(Entity, bool)> = world
            .query_ref::<(Entity, &DataAssetHolder)>()
            .into_iter()
            .map(|(holder, kept)| (holder, shown.contains(&holder) && db.file(&kept.path).is_file()))
            .collect();
        for (holder, kept) in holders {
            match kept {
                true => self.save_data_asset(holder),
                false => drop(self.game.engine.world.despawn(holder)),
            }
        }
        if let Some(world) = self.play.edit_world() {
            let aside: Vec<Entity> =
                world.query_ref::<(Entity, &DataAssetHolder)>().into_iter().map(|(e, _)| e).collect();
            for holder in aside {
                world.despawn(holder);
            }
        }
    }

    /// Writes a data asset's fields into its file when they changed since
    /// they were last read or written, and tells the file watcher it has
    /// seen it, so the asset is not read again over what is being typed.
    fn save_data_asset(&mut self, holder: Entity) {
        let world = &self.game.engine.world;
        let (Some(kept), Some((type_name, fields))) =
            (world.get::<DataAssetHolder>(holder), data_assets::fields_of(world, holder))
        else {
            return;
        };
        if *fields == kept.written {
            return;
        }
        let path = kept.path.clone();
        let file = self.game.content.db().file(&path);
        let written = data_assets::text(&type_name, &fields).and_then(|text| Ok(std::fs::write(&file, text)?));
        if let Err(error) = written {
            return self.notify(&format!("Could not save {path}: {error:#}"));
        }
        self.files_seen();
        let before = match self.game.engine.world.get_mut::<DataAssetHolder>(holder) {
            Some(kept) => std::mem::replace(&mut kept.written, (*fields).clone()),
            None => return,
        };
        self.refill_loaded(&path, &before, &fields);
    }

    /// Gives the scripts' instance of the data asset at `path`, when they
    /// have loaded it, the fields the Inspector changed from `before` to
    /// `after`: a game playing sees the new values at once, as does code
    /// run while editing, and fields the change left alone keep what the
    /// game's code set on them.
    fn refill_loaded(&mut self, path: &str, before: &toml::Table, after: &toml::Table) {
        let Some(guid) = self.game.content.db().guid_of(path) else { return };
        let changed: toml::Table = after
            .iter()
            .filter(|&(name, value)| before.get(name) != Some(value))
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        let reset: Vec<String> = before.keys().filter(|&name| !after.contains_key(name)).cloned().collect();
        let world = &mut self.game.engine.world;
        let refilled = match self.play.edit_scripts() {
            Some(scripts) => scripts.refill_data_asset(world, &guid.to_string(), &changed, &reset),
            None => self.game.refill_data_asset(guid, &changed, &reset),
        };
        if let Err(error) = refilled {
            tracing::error!("{error:#}");
        }
    }

    /// Opens the object picker under `anchor` with the scripts' Definitions,
    /// each where its `[CreateAsset]` path puts it, or else under Scripts by
    /// its namespace, to make a data asset of the one picked in `folder`.
    pub(super) fn browse_definitions(&mut self, folder: String, anchor: NodeId) {
        let definitions = &self.inspector.types().definitions;
        let shared = shared_namespace(definitions.iter());
        let choices: Vec<Choice> = definitions
            .iter()
            .filter_map(|definition| {
                let (place, _) = menu_place(definition, &shared)?;
                Some(Choice {
                    label: data_assets::new_name(definition),
                    detail: String::new(),
                    icon: "file",
                    pick: Chosen::Definition(definition.name.clone()),
                    preview: None,
                    folder: place,
                })
            })
            .collect();
        if choices.is_empty() {
            return self.notify("The scripts have no Definition to make a data asset of");
        }
        self.picker.open(&mut self.ui, (PickFor::NewDataAsset { folder }, anchor), "New Data Asset", choices, "");
    }

    /// Makes a data asset of the Definition `type_name` in `folder`, and
    /// selects it, ready to rename.
    pub(super) fn make_data_asset(&mut self, folder: &str, type_name: &str, renderer: &mut Renderer) {
        let Some(definition) = self.inspector.types().definition(type_name).cloned() else { return };
        let made = crate::project_files::place(self.game.content.db().mounts(), folder)
            .and_then(|at| data_assets::create(&at.root, &at.path, &definition).map(|made| at.back(&made)));
        match made {
            Ok(made) => self.project_event(ProjectEvent::Changed { select: Some(made), rename: true }, renderer),
            Err(error) => self.notify(&format!("{error:#}")),
        }
    }
}
