//! What the editor shows of dimensions, worked out from the game: the
//! dimensions a Scene view can show, where a Portal leads when that is into
//! another dimension, and the Portals of the project's `.dimension` files
//! that a Portal's Leads To can pick.

use voxear_assets::{AssetGuid, AssetKind};
use voxear_ecs::{Entity, World};
use voxear_engine::{
    Content, Game, ObjPath,
    dimensions::{self, DimensionFile, DimensionId, Dimensions},
    dungeons::{DungeonGate, DungeonWayOut},
    portals::PortalInto,
};
use voxear_render::Portal;

/// The dimensions a Scene view can show: the level's first, then each one
/// made, by its number, each with its name.
pub fn listed(world: &World, content: &Content) -> Vec<(DimensionId, String)> {
    let mut list = vec![(DimensionId::LEVEL, dimensions::name_of(world, content, DimensionId::LEVEL))];
    if let Some(made) = world.resource::<Dimensions>() {
        let mut ids: Vec<DimensionId> = made.made().map(|made| made.id).collect();
        ids.sort_by_key(|id| id.0);
        list.extend(ids.into_iter().map(|id| (id, dimensions::name_of(world, content, id))));
    }
    list
}

/// A Portal in a `.dimension` file, as a Portal's Leads To picks it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilePortal {
    pub dimension: AssetGuid,
    /// The file's path in the project, as a Portal's `into` names it.
    pub file: String,
    pub portal: ObjPath,
    pub name: String,
}

impl FilePortal {
    /// What its picker lists it as: its name, then its dimension's.
    pub fn label(&self) -> String {
        format!("{} · {}", self.name, stem(&self.file))
    }
}

/// Every Portal of the project's `.dimension` files, by file then by its
/// place in the file.
pub fn file_portals(content: &Content) -> Vec<FilePortal> {
    let db = content.db();
    let mut files: Vec<(AssetGuid, String)> =
        db.entries().filter(|entry| entry.kind == AssetKind::Dimension).map(|e| (e.guid, e.path.clone())).collect();
    files.sort_by(|a, b| a.1.cmp(&b.1));
    let mut found = Vec::new();
    for (dimension, file) in files {
        let portals = portals_in(&db.file(&file));
        found.extend(portals.into_iter().map(|(portal, name)| FilePortal {
            dimension,
            file: file.clone(),
            portal,
            name,
        }));
    }
    found
}

/// The Portals of a dimension's file, by their place in it, with their
/// names: read again only once the file has changed, as a selected
/// Portal's label and its Leads To ask every frame.
fn portals_in(file: &std::path::Path) -> Vec<(ObjPath, String)> {
    type Read = (Option<std::time::SystemTime>, Vec<(ObjPath, String)>);
    thread_local! {
        static READ: std::cell::RefCell<std::collections::HashMap<std::path::PathBuf, Read>> = Default::default();
    }
    let changed = std::fs::metadata(file).and_then(|meta| meta.modified()).ok();
    if let Some(kept) =
        READ.with_borrow(|read| read.get(file).filter(|(at, _)| *at == changed).map(|(_, kept)| kept.clone()))
    {
        return kept;
    }
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let portals: Vec<(ObjPath, String)> = DimensionFile::parse(&text, "")
        .map(|read| {
            let objs = read.scene.objs.into_iter().filter(|obj| obj.portal.is_some());
            objs.map(|obj| (ObjPath::new(obj.id), obj.name)).collect()
        })
        .unwrap_or_default();
    READ.with_borrow_mut(|read| read.insert(file.to_path_buf(), (changed, portals.clone())));
    portals
}

/// The same, with those of each dimension's file open in the editor as it
/// is now, saved or not, in place of its file's.
pub fn file_portals_of(game: &Game) -> Vec<FilePortal> {
    let world = &game.engine.world;
    let open = game.levels.open_dimensions();
    let mut found: Vec<FilePortal> = file_portals(&game.content)
        .into_iter()
        .filter(|far| !open.iter().any(|(guid, ..)| *guid == far.dimension))
        .collect();
    for (dimension, file, _) in open {
        for (entity, portal) in game.levels.tracked_in(dimension) {
            if !world.has::<Portal>(entity) || portal.ids().is_empty() {
                continue;
            }
            let name = world.get::<voxear_scene::Name>(entity).map_or_else(|| "Portal".into(), |name| name.0.clone());
            found.push(FilePortal { dimension, file: file.to_string(), portal, name });
        }
    }
    found.sort_by(|a, b| (&a.file, a.portal.ids()).cmp(&(&b.file, b.portal.ids())));
    found
}

/// The Portal of a dimension's file a Portal's `into` names, as listed.
pub fn into_of(game: &Game, portal: Entity) -> Option<FilePortal> {
    let into = game.engine.world.get::<PortalInto>(portal)?;
    let dimension = game.content.parse(&into.0.dimension).ok()?.guid()?;
    let path = into.0.portal.path().ok()?;
    let found = file_portals(&game.content).into_iter().find(|f| f.dimension == dimension && f.portal == path);
    found.or_else(|| {
        let file = game.content.db().get(dimension)?.path.clone();
        Some(FilePortal { dimension, file, portal: path, name: "Portal".into() })
    })
}

/// The Portal one leads to where it is in another dimension that is made
/// now: where it leads, a dimension's Portal in a level's file that leads
/// into it where it leads nowhere itself, or for a dimension's file open in
/// the editor, its Portal that `into` names.
pub fn far_portal(game: &Game, portal: Entity) -> Option<Entity> {
    let world = &game.engine.world;
    let here = dimensions::dimension_of(world, portal);
    let to = world.get::<Portal>(portal)?.to.filter(|to| world.is_alive(*to));
    if let Some(to) = to.filter(|to| dimensions::dimension_of(world, *to) != here) {
        return Some(to);
    }
    if let Some(into) = into_of(game, portal) {
        return game.levels.entity_at(into.dimension, &into.portal);
    }
    let (scene, path) = game.levels.place_of(portal)?;
    game.levels.dimension_of_scene(scene).filter(|id| !id.is_level())?;
    let leading: Vec<(Entity, PortalInto)> =
        world.query_ref::<(Entity, &PortalInto)>().into_iter().map(|(entity, into)| (entity, into.clone())).collect();
    leading
        .into_iter()
        .map(|(entity, _)| entity)
        .find(|&entity| into_of(game, entity).is_some_and(|into| into.dimension == scene && into.portal == path))
}

/// Whether a Portal leads into another dimension: into a dimension's file,
/// to a Portal in another dimension, out of a dungeon, or from a dimension's
/// file back to the Portal that leads into it.
pub fn leads_elsewhere(game: &Game, portal: Entity) -> bool {
    let world = &game.engine.world;
    world.has::<PortalInto>(portal) || world.has::<DungeonWayOut>(portal) || far_portal(game, portal).is_some()
}

/// The name of the dimension a Portal leads into, where it is another than
/// its own: as the Scene view's gizmo labels it.
pub fn far_name(game: &Game, portal: Entity) -> Option<String> {
    let world = &game.engine.world;
    if let Some(far) = far_portal(game, portal) {
        return Some(dimensions::name_of(world, &game.content, dimensions::dimension_of(world, far)));
    }
    if let Some(into) = into_of(game, portal) {
        return Some(stem(&into.file).to_string());
    }
    if world.has::<DungeonGate>(portal) {
        return Some("Dungeon".into());
    }
    world.has::<DungeonWayOut>(portal).then(|| dimensions::name_of(world, &game.content, DimensionId::LEVEL))
}

/// A file's name without its folder or extension.
pub fn stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}
