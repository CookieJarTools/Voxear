//! Dimensions in the editor. A `.dimension` file opens as a scene does,
//! alongside the level, its Objs in a dimension of their own under a header
//! of their own in the Hierarchy, saved back to its file with its settings.
//! A dungeon's dimension is opened from its gate, made by code as the game
//! makes it, and not saved.
//!
//! Each Scene view shows one dimension at a time, picked from the Dimension
//! menu in its header, and keeps where its camera was in each, so going
//! back to one is going back to where it was looked from. Selecting an Obj
//! of another dimension shows that dimension. The game's sky, sun and
//! weather are those of the dimension the Scene view in use shows
//! (`ViewerDimension`). A Portal into another dimension goes through to the
//! Portal it leads to (Go Through), and a dungeon's gate opens its dungeon
//! (Open Dungeon).

use voxear_assets::{AssetGuid, AssetKind};
use voxear_core::math::{Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_engine::{
    dimensions::{self, DimensionFile, DimensionId, DimensionSettings, ViewerDimension},
    dungeons::DungeonGate,
};
use voxear_render::{Portal, PortalPose, Renderer};
use voxear_scene::{Destroying, Parent, Transform};

use super::{Editor, scene_views::SceneView};
use crate::dimension_views;

/// How far in front of a Portal the Scene view's camera goes to show it,
/// as a share of its opening's height, and how far at the least, in metres.
const IN_FRONT: f32 = 1.6;
const IN_FRONT_LEAST: f32 = 4.0;

impl Editor {
    /// Each frame: keeps each open scene's Objs in its dimension, each Scene
    /// view's Dimension menu listing those made, a view of one let go back
    /// in the level's, and the game seen from the dimension the Scene view
    /// in use shows.
    pub(super) fn update_dimensions(&mut self) {
        if !self.is_playing() {
            self.game.levels.keep_scene_dimensions(&mut self.game.engine.world);
        }
        let listed = dimension_views::listed(&self.game.engine.world, &self.game.content);
        let mut wanted = None;
        for index in 0..=self.scene_views.len() {
            let view = match index {
                0 => &mut self.scene,
                _ => &mut self.scene_views[index - 1],
            };
            let world = &mut self.game.engine.world;
            if !listed.iter().any(|(id, _)| *id == dimensions::dimension_of(world, view.camera)) {
                show_in(view, world, DimensionId::LEVEL);
            }
            let shown = dimensions::dimension_of(world, view.camera);
            if view.listed != listed {
                let names: Vec<&str> = listed.iter().map(|(_, name)| name.as_str()).collect();
                self.ui.set_options(view.dimension_menu, &names);
                view.listed = listed.clone();
            }
            self.ui.set_visible(view.dimension_menu, listed.len() > 1);
            let at = listed.iter().position(|(id, _)| *id == shown).unwrap_or(0);
            if self.ui.changed(view.dimension_menu) {
                let picked = listed.get(self.ui.selected_option(view.dimension_menu)).map(|(id, _)| *id);
                wanted = picked.map(|id| (index, id));
            } else if self.ui.selected_option(view.dimension_menu) != at {
                self.ui.set_selected_option(view.dimension_menu, at);
            }
        }
        if let Some((index, id)) = wanted {
            let view = match index {
                0 => &mut self.scene,
                _ => &mut self.scene_views[index - 1],
            };
            show_in(view, &mut self.game.engine.world, id);
        }
        self.seen_from_scene_view();
    }

    /// The game is seen from the dimension the Scene view in use shows: set
    /// as soon as that changes, so the next frame makes ground and stands
    /// gates round its camera in the dimension it is in.
    pub(super) fn seen_from_scene_view(&mut self) {
        let world = &mut self.game.engine.world;
        let viewer = dimensions::dimension_of(world, self.scene.camera);
        world.insert_resource(ViewerDimension(viewer));
    }

    /// The name of the dimension a selected Portal leads into, over its
    /// opening in the Scene view, where that is another dimension than its
    /// own. Drawn as debug draw's text, for the frame.
    pub(super) fn label_far_dimensions(&mut self) {
        let Some(portal) = self.selection else { return };
        let world = &self.game.engine.world;
        let Some(size) = world.get::<Portal>(portal).map(|portal| portal.size) else { return };
        if dimensions::dimension_of(world, portal) != dimensions::dimension_of(world, self.scene.camera) {
            return;
        }
        let Some(name) = dimension_views::far_name(&self.game, portal) else { return };
        let Some(pose) = PortalPose::of_obj(world, portal) else { return };
        let above = pose.position + pose.rotation * Vec3::new(0.0, size.y + 0.4, 0.0);
        voxear_core::debug_draw::text(above, &format!("→ {name}"), [0.42, 0.62, 1.0, 1.0], 0.0);
    }

    /// The scene a new Obj goes into where nothing says: the file of the
    /// dimension the Scene view in use shows, where that is one open in the
    /// editor; else the active scene.
    pub(super) fn scene_for_new_objs(&self) -> Option<AssetGuid> {
        let shown = self.scene_dimension();
        (!shown.is_level()).then(|| self.game.levels.dimension_scene(shown)).flatten()
    }

    /// The dimension the Scene view in use shows.
    pub fn scene_dimension(&self) -> DimensionId {
        dimensions::dimension_of(&self.game.engine.world, self.scene.camera)
    }

    /// Shows a dimension in the Scene view in use, from where it was last
    /// looked at from in it. False where there is no such dimension.
    pub fn show_dimension(&mut self, id: DimensionId) -> bool {
        let world = &mut self.game.engine.world;
        let known = world.resource::<dimensions::Dimensions>().is_some_and(|made| made.exists(id));
        if !(known || id.is_level()) {
            return false;
        }
        show_in(&mut self.scene, world, id);
        self.glide = None;
        self.seen_from_scene_view();
        true
    }

    /// Shows the dimension an Obj is in, where the Scene view in use shows
    /// another: as selecting it does.
    pub(super) fn show_dimension_of(&mut self, entity: Entity) {
        let world = &self.game.engine.world;
        if !world.is_alive(entity) || entity == self.scene.camera {
            return;
        }
        let id = dimensions::dimension_of(world, entity);
        if id != self.scene_dimension() {
            self.show_dimension(id);
        }
    }

    /// Opens a `.dimension` file: alongside the level while editing, its
    /// Objs to edit and save; while playing, made as a script makes one.
    /// Returns its dimension, which the Scene view in use then shows.
    pub fn open_dimension(&mut self, file: AssetGuid, renderer: &mut Renderer) -> Result<DimensionId, String> {
        let db = self.game.content.db();
        let entry = db.get(file).ok_or_else(|| format!("no asset with GUID {file}"))?;
        if entry.kind != AssetKind::Dimension {
            return Err(format!("{} is not a .dimension file", entry.path));
        }
        let path = entry.path.clone();
        let id = match self.is_playing() {
            true => self.game.make_dimension(file, 0, Some(renderer)).map_err(|error| format!("{error:#}"))?,
            false => {
                if self.prefab.is_some() {
                    return Err("go back from the prefab to open a dimension".into());
                }
                let game = &mut self.game;
                game.levels
                    .open_scene_alongside(&mut game.engine, &mut game.content, Some(renderer), file)
                    .map_err(|error| format!("could not open {path}: {error:#}"))?;
                game.levels.dimension_of_scene(file).ok_or_else(|| format!("{path} did not open"))?
            }
        };
        self.refresh_hierarchies();
        self.show_dimension(id);
        Ok(id)
    }

    /// Lets a dimension go: a `.dimension` file open in the editor closes,
    /// which with edits not saved needs `discard`; one made by code, as a
    /// dungeon's, goes with its Objs. The Scene views showing it go back to
    /// the level's.
    pub fn let_go_of_dimension(&mut self, id: DimensionId, discard: bool) -> Result<(), String> {
        if id.is_level() {
            return Err("the level's own dimension stays while the level is open".into());
        }
        if !self.game.engine.world.resource::<dimensions::Dimensions>().is_some_and(|made| made.exists(id)) {
            return Err(format!("no dimension {} is made", id.0));
        }
        if let Some(scene) = self.game.levels.dimension_scene(id).filter(|_| !self.is_playing()) {
            let game = &mut self.game;
            let root = game.project.root.clone();
            if !discard && !game.levels.scene_matches_disk(&game.engine, &mut game.content, &root, scene) {
                return Err("its file has edits not saved; save, or let it go without them".into());
            }
            game.levels.close_scene(&mut game.engine, scene).map_err(|error| format!("{error:#}"))?;
            // Steps that name its Objs cannot be undone any more.
            self.history.clear();
        } else {
            self.game.let_go_dimension(id);
        }
        let selected = self.selected().filter(|&entity| {
            let world = &self.game.engine.world;
            world.is_alive(entity) && !world.has::<Destroying>(entity) && dimensions::dimension_of(world, entity) != id
        });
        self.select(selected);
        for view in std::iter::once(&mut self.scene).chain(&mut self.scene_views) {
            if dimensions::dimension_of(&self.game.engine.world, view.camera) == id {
                show_in(view, &mut self.game.engine.world, DimensionId::LEVEL);
            }
        }
        self.seen_from_scene_view();
        self.refresh_hierarchies();
        Ok(())
    }

    /// Makes the dimension of a dungeon whose gate is `gate` (a Portal
    /// standing in it, or the dungeon found by its site), links the gate's
    /// Portal and the way out both ways, and shows the way out in the Scene
    /// view in use. Returns the dimension and the way out.
    pub fn open_dungeon(
        &mut self,
        gate: DungeonGate,
        renderer: &mut Renderer,
    ) -> Result<(DimensionId, Entity), String> {
        let (id, back) =
            self.game.make_dungeon_dimension(gate, Some(renderer)).map_err(|error| format!("{error:#}"))?;
        let world = &mut self.game.engine.world;
        let standing: Option<Entity> = world
            .query_ref::<(Entity, &DungeonGate)>()
            .without::<Destroying>()
            .into_iter()
            .find(|(_, standing)| **standing == gate)
            .map(|(entity, _)| entity);
        if let Some(portal) = standing {
            if let Some(near) = world.get_mut::<Portal>(portal) {
                near.to = Some(back);
            }
            if let Some(far) = world.get_mut::<Portal>(back) {
                far.to = Some(portal);
            }
        }
        self.refresh_hierarchies();
        self.show_portal(back);
        self.select(Some(back));
        Ok((id, back))
    }

    /// Shows the Portal one leads to in the Scene view in use, in its own
    /// dimension, and selects it: a dungeon's gate opens its dungeon first,
    /// and a Portal into a dimension's file opens that file. Returns the far
    /// Portal.
    pub fn go_through(&mut self, portal: Entity, renderer: &mut Renderer) -> Result<Entity, String> {
        let world = &self.game.engine.world;
        if !world.has::<Portal>(portal) {
            return Err("the Obj has no Portal".into());
        }
        let far = dimension_views::far_portal(&self.game, portal);
        let far = match far {
            Some(far) => far,
            None => {
                if let Some(gate) = world.get::<DungeonGate>(portal).copied() {
                    return self.open_dungeon(gate, renderer).map(|(_, back)| back);
                }
                let into = dimension_views::into_of(&self.game, portal);
                let into = into.ok_or_else(|| "the Portal leads into no other dimension".to_string())?;
                self.open_dimension(into.dimension, renderer)?;
                dimension_views::far_portal(&self.game, portal)
                    .ok_or_else(|| format!("{} has no Portal {}", into.file, into.portal))?
            }
        };
        self.show_portal(far);
        self.select(Some(far));
        Ok(far)
    }

    /// Shows a Portal from in front of it, in its dimension.
    fn show_portal(&mut self, portal: Entity) {
        let world = &mut self.game.engine.world;
        let id = dimensions::dimension_of(world, portal);
        show_in(&mut self.scene, world, id);
        if let Some(place) = in_front_of(world, portal) {
            put_camera(world, self.scene.camera, place);
        }
        self.glide = None;
        self.seen_from_scene_view();
    }

    /// Writes a `.dimension` file's settings, the rest of the file as it is
    /// on disk; where it is open, its dimension takes them at once and its
    /// next save keeps them.
    pub fn save_dimension_settings(&mut self, path: &str, settings: DimensionSettings) -> Result<(), String> {
        let db = self.game.content.db();
        let guid = db.guid_of(path).ok_or_else(|| format!("{path} is not in the project"))?;
        let file = db.file(path);
        let text = std::fs::read_to_string(&file).map_err(|error| error.to_string())?;
        let mut read = DimensionFile::parse(&text, path).map_err(|error| format!("{error:#}"))?;
        read.settings = settings.clone();
        let text = read.to_toml().map_err(|error| format!("{error:#}"))?;
        std::fs::write(&file, text).map_err(|error| error.to_string())?;
        self.files_seen();
        let game = &mut self.game;
        game.levels
            .set_dimension_settings(&mut game.engine, &mut game.content, guid, settings)
            .map_err(|error| format!("{error:#}"))?;
        Ok(())
    }

    /// Moves a root Obj, everything under it with it, into a dimension, at
    /// `at` there or where it is. While editing, an Obj moves between the
    /// level and the files open as dimensions, into the file of the one it
    /// goes into; a dimension made by code, as a dungeon's, takes Objs only
    /// while playing, as nothing of it is saved.
    pub fn move_to_dimension(&mut self, obj: Entity, id: DimensionId, at: Option<Vec3>) -> Result<(), String> {
        let world = &self.game.engine.world;
        if !world.is_alive(obj) || world.has::<voxear_scene::EditorOnly>(obj) {
            return Err("no such Obj".into());
        }
        if world.has::<Parent>(obj) {
            return Err("only an Obj at the top moves to another dimension; everything under it goes with it".into());
        }
        if !(id.is_level() || world.resource::<dimensions::Dimensions>().is_some_and(|made| made.exists(id))) {
            return Err(format!("no dimension {} is made", id.0));
        }
        if !self.is_playing() {
            let scene = match id.is_level() {
                true => self.game.levels.active_scene_guid(),
                false => self.game.levels.dimension_scene(id),
            };
            let scene = scene.ok_or_else(|| {
                "a dimension made by code, as a dungeon's, is not saved: open a .dimension file to move Objs into"
                    .to_string()
            })?;
            let moving = objs_under(&self.game.engine.world, obj);
            self.game.levels.move_objs(&moving, scene, None);
        }
        let world = &mut self.game.engine.world;
        dimensions::move_obj(world, obj, id);
        if let Some(at) = at
            && let Some(transform) = world.get_mut::<Transform>(obj)
        {
            transform.position = at;
        }
        self.unsaved |= !self.is_playing();
        self.refresh_hierarchies();
        Ok(())
    }
}

/// An Obj and every Obj under it, it first.
fn objs_under(world: &World, root: Entity) -> Vec<Entity> {
    let mut all = vec![root];
    let mut at = 0;
    while at < all.len() {
        let parent = all[at];
        let children = world.query_ref::<(Entity, &Parent)>().into_iter().filter(|(_, p)| p.0 == parent);
        all.extend(children.map(|(child, _)| child).collect::<Vec<_>>());
        at += 1;
    }
    all
}

/// Shows `to` in a Scene view: where its camera was looked from there last,
/// or in front of the first Portal in it, or by its first Obj. The place
/// left is kept for coming back.
fn show_in(view: &mut SceneView, world: &mut World, to: DimensionId) {
    let from = dimensions::dimension_of(world, view.camera);
    if from == to {
        return;
    }
    if let Some(place) = world.get::<Transform>(view.camera).copied() {
        view.places.insert(from, place);
    }
    dimensions::set_dimension(world, view.camera, to);
    let place = view.places.get(&to).copied().or_else(|| first_look(world, to));
    if let Some(place) = place {
        put_camera(world, view.camera, place);
    }
}

fn put_camera(world: &mut World, camera: Entity, place: Transform) {
    if let Some(transform) = world.get_mut::<Transform>(camera) {
        (transform.position, transform.rotation) = (place.position, place.rotation);
    }
}

/// Where a Scene view first looks at a dimension from: in front of its
/// first Portal, or back from its first Obj.
fn first_look(world: &World, id: DimensionId) -> Option<Transform> {
    let mut portals: Vec<Entity> = world
        .query_ref::<(Entity, &Portal)>()
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, _)| entity)
        .filter(|&entity| dimensions::dimension_of(world, entity) == id)
        .collect();
    portals.sort_by_key(|entity| entity.index());
    if let Some(place) = portals.first().and_then(|&portal| in_front_of(world, portal)) {
        return Some(place);
    }
    let mut roots: Vec<Entity> = world
        .query_ref::<(Entity, &voxear_scene::InDimension)>()
        .without::<voxear_scene::EditorOnly>()
        .into_iter()
        .filter(|(_, own)| own.0 == id)
        .map(|(entity, _)| entity)
        .collect();
    roots.sort_by_key(|entity| entity.index());
    let at = voxear_scene::world_matrix(world, *roots.first()?).w_axis.truncate();
    let back = Vec3::new(0.0, 8.0, -14.0);
    Some(Transform::new(at + back, looking(-back), Vec3::ONE))
}

/// In front of a Portal, at the height of its middle, looking at it.
fn in_front_of(world: &World, portal: Entity) -> Option<Transform> {
    let size = world.get::<Portal>(portal)?.size;
    let pose = PortalPose::of_obj(world, portal)?;
    let middle = pose.position + pose.rotation * Vec3::new(0.0, size.y * 0.5, 0.0);
    let at = middle + pose.front() * (size.y * IN_FRONT).max(IN_FRONT_LEAST);
    Some(Transform::new(at, looking(middle - at), Vec3::ONE))
}

/// A Scene camera's turn to look along `direction`: it looks along its +Z.
fn looking(direction: Vec3) -> Quat {
    let along = direction.normalize_or(Vec3::Z);
    let yaw = along.x.atan2(along.z);
    let pitch = (-along.y).clamp(-1.0, 1.0).asin();
    Quat::from_euler(voxear_core::math::EulerRot::YXZ, yaw, pitch, 0.0)
}
