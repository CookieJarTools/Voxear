//! A VoxDocs guide's steps, answered by the editor. An action's button runs
//! the command it names by its title, as the palette would run it: the
//! editor's own, such as `action:Play` or `action:New Light`, or one a
//! script adds with `[Command]`. A check looks at the edited level or the
//! project, and ticks itself while what it wants is there:
//!
//! - `check:obj/Sun`: there is an Obj called Sun.
//! - `check:component/Light`: some Obj has a Light, or a behaviour of that
//!   name, as `Spinner` or `Hello.Spinner`.
//! - `check:file/Prefabs/Lamp.prefab`: the project has that file.

use voxear_ecs::Entity;
use voxear_render::Renderer;
use voxear_scene::Name;

use super::Editor;
use crate::{
    hierarchy::EditorOnly,
    palette::{self, Sources},
    props::{self, Part},
};

impl Editor {
    pub(super) fn update_docs_steps(&mut self, renderer: &mut Renderer) {
        if let Some(title) = self.docs.take_action_request() {
            let sources = Sources {
                game: &self.game,
                playing: self.is_playing(),
                settings: &self.memory.settings,
                recent: &self.memory.recent,
                docs: Vec::new(),
                added: self.added_commands(),
            };
            match palette::command_named(&sources, &title) {
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

    /// What a guide's check finds: nothing wrong, or what is still to do.
    fn check(&self, check: &str) -> Result<(), String> {
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
