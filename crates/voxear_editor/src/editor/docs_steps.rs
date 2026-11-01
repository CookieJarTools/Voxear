//! A VoxDocs guide's steps, answered by the editor. An action's button runs
//! the command it names by its title, as the palette would run it: the
//! editor's own, such as `action:Play` or `action:New Cube`, or one a
//! script adds with `[Command]`. A check looks at the edited level or the
//! project, and ticks itself while what it wants is there:
//!
//! - `check:obj/Sun`: there is an Obj called Sun.
//! - `check:component/Light`: some Obj has a Light, or a behaviour of that
//!   name, as `Spinner` or `Hello.Spinner`.
//! - `check:file/Prefabs/Lamp.prefab`: the project has that file.
//!
//! An action can also make an asset, as the Project panel's New menu does:
//! `action:create/Material` beside the project's others, under a name of its
//! own, or `<action:create/Material?at=Materials/Brass.material>` just there,
//! found rather than made again if it is there already. Folder, Scene,
//! Level, Material and Script can be made; what is made shows in the
//! Project panel.
//!
//! While a page is written, its Action… menu lists the same commands, to
//! put a step in at the cursor.

use anyhow::{Context, Result};
use voxear_ecs::Entity;
use voxear_render::Renderer;
use voxear_scene::Name;

use super::Editor;
use crate::{
    hierarchy::EditorOnly,
    palette::{self, Sources},
    project_files::{self, NewFile},
    project_view::ProjectEvent,
    props::{self, Part},
};

/// What a guide's step that makes an asset starts with, after `action:`.
const CREATE: &str = "create/";

impl Editor {
    pub(super) fn update_docs_steps(&mut self, renderer: &mut Renderer) {
        if self.docs.take_commands_wanted() {
            let titles = palette::command_titles(&self.command_sources());
            self.docs.open_action_menu(&mut self.ui, &titles);
        }
        if let Some(title) = self.docs.take_action_request() {
            if let Some(what) = title.strip_prefix(CREATE) {
                match self.create_from_guide(what, renderer) {
                    Ok(()) => self.docs.action_ran(&mut self.ui),
                    Err(error) => self.notify(&format!("{error:#}")),
                }
                return;
            }
            match palette::command_named(&self.command_sources(), &title) {
                Some(action) => {
                    self.run_palette(action, renderer);
                    self.docs.action_ran(&mut self.ui);
                }
                None => self.notify(&format!("There is no command called {title}")),
            }
        }
        for check in self.docs.wanted_checks() {
            let found = self.check(&check);
            self.docs.set_check(&mut self.ui, &check, found);
        }
    }

    /// A guide's `create/` step: makes the asset it names where it says, or
    /// beside the others, and shows it in the Project panel.
    fn create_from_guide(&mut self, what: &str, renderer: &mut Renderer) -> Result<()> {
        let (kind, at) = what.split_once("?at=").map_or((what, None), |(kind, at)| (kind, Some(at)));
        let new = NewFile::ALL
            .into_iter()
            .find(|new| new.label().rsplit(' ').next().is_some_and(|word| word.eq_ignore_ascii_case(kind)))
            .with_context(|| {
                format!("A guide cannot make a {kind}: it makes a Folder, Scene, Level, Material or Script")
            })?;
        let root = self.game.project.root.clone();
        let namespace = super::script_namespace(&self.game.project);
        let path = match at {
            Some(at) if root.join(at).exists() => at.to_string(),
            Some(at) => {
                let (folder, name) = at.rsplit_once('/').unwrap_or(("", at));
                std::fs::create_dir_all(root.join(folder)).with_context(|| format!("making {folder}"))?;
                let made = project_files::create(&root, folder, new, &namespace)?;
                project_files::rename(&root, &made, name)?
            }
            None => project_files::create(&root, "", new, &namespace)?,
        };
        self.show_panel("project");
        self.project_event(ProjectEvent::Changed { select: Some(path), rename: false }, renderer);
        Ok(())
    }

    /// What the palette's commands come from, as a guide finds them: the
    /// editor's own and the scripts'.
    fn command_sources(&self) -> Sources<'_> {
        Sources {
            game: &self.game,
            playing: self.is_playing(),
            settings: &self.memory.settings,
            recent: &self.memory.recent,
            docs: Vec::new(),
            added: self.added_commands(),
        }
    }

    /// What a guide's check finds: nothing wrong, or what is still to do.
    fn check(&self, check: &str) -> std::result::Result<(), String> {
        let world = &self.game.engine.world;
        let objs: Vec<Entity> =
            world.query_ref::<(Entity, &Name)>().without::<EditorOnly>().into_iter().map(|(e, _)| e).collect();
        let (kind, wanted) = check.split_once('/').unwrap_or((check, ""));
        let found = match kind {
            "obj" => objs.iter().any(|&e| world.get::<Name>(e).is_some_and(|name| name.0 == wanted)),
            "component" => objs.iter().any(|&e| props::parts(world, e).iter().any(|part| names(part, wanted))),
            "file" => !wanted.is_empty() && self.game.project.root.join(wanted).exists(),
            _ => return Err(format!("VoxDocs has no check called {check}")),
        };
        if found {
            return Ok(());
        }
        Err(match kind {
            "obj" => format!("There is no Obj called {wanted} yet"),
            "component" => format!("No Obj has a {wanted} yet"),
            _ => format!("There is no {wanted} in the project yet"),
        })
    }
}

/// Whether a component is the one a check names: by its title, as `Light`
/// or `Spinner`, or a behaviour by its full name.
fn names(part: &Part, wanted: &str) -> bool {
    part.title().eq_ignore_ascii_case(wanted) || part.lock_key().eq_ignore_ascii_case(wanted)
}
