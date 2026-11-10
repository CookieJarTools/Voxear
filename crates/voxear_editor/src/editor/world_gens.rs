//! World Generators in the Inspector (see `world_gen_inspector`) and the
//! Generator panel (see `generators`): saved as they are changed, and every
//! terrain in the open level with a Generate layer of theirs made again at
//! once, so the ground in the Scene view follows each change.

use voxear_terrain::generate::WorldGen;

use super::Editor;
use crate::project_files::NEW_WORLD_GEN;

impl Editor {
    /// Writes a World Generator, and makes the terrains and block worlds
    /// generated from it again where the ground it makes changed.
    pub(super) fn save_world_gen(&mut self, path: &str, world: &WorldGen) {
        let text = format!("{NEW_WORLD_GEN}{}", world.to_text());
        if let Err(error) = std::fs::write(self.game.project.root.join(path), text) {
            return self.notify(&format!("Could not save {path}: {error}"));
        }
        self.files_seen();
        if let Some(guid) = self.game.content.db().guid_of(path)
            && self.game.content.forget_world_gen(guid)
        {
            crate::terrain_props::regenerate(&mut self.game, guid);
            crate::block_props::regenerate(&mut self.game, guid);
        }
    }
}
