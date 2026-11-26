//! The rows of an endless Voxel Volume's Inspector section for what grows
//! and walks on it: each kind of tree's World Areas, its crops and ground
//! cover, and the mobs spawned on it. A tag row picks one Area Tag, the
//! easy way; a scene, or `vox editor call`, can give a rule several.

use voxear_assets::{AssetGuid, AssetKind};
use voxear_ecs::Entity;
use voxear_engine::{
    Game,
    script_types::ScriptTypes,
    voxel_mobs::VoxelMobsData,
    voxel_volumes::{VoxelCoverData, VoxelVolumeData, VoxelVolumeSource},
};
use voxear_render::{MOST_COVER_KINDS, VoxelCoverKind, VoxelMaterial};

use crate::{
    fields::{self, Pick},
    props::Value,
};

/// The Definition of tags' data assets.
const AREA_TAG: &str = "Voxear.AreaTag";
/// The most kinds of mob the Inspector adds to one volume.
const MOST_MOB_KINDS: usize = 16;

/// A row of the section, with the kind of tree, cover or mob it is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GrowthField {
    TreeWithin(usize),
    TreeNever(usize),
    CoverKind(usize),
    CoverOn(usize),
    CoverSoil(usize),
    CoverHeight(usize),
    CoverRows(usize),
    CoverDensity(usize),
    CoverWithin(usize),
    CoverNever(usize),
    CoverAdd,
    MobPrefab(usize),
    MobPerHectare(usize),
    MobMost(usize),
    MobOn(usize),
    MobNear(usize),
    MobFar(usize),
    MobWithin(usize),
    MobNever(usize),
    MobAdd,
}

use GrowthField as G;

/// The rows a kind of tree has after its plant and its density.
pub fn tree_rows(kind: usize) -> [GrowthField; 2] {
    [G::TreeWithin(kind), G::TreeNever(kind)]
}

/// The rows for the cover and the mobs of an endless volume written so.
pub fn fields(data: &VoxelVolumeData) -> Vec<GrowthField> {
    let mut rows = Vec::new();
    for k in 0..data.cover.len() {
        rows.extend([G::CoverKind(k), G::CoverOn(k), G::CoverSoil(k), G::CoverHeight(k), G::CoverRows(k)]);
        rows.extend([G::CoverDensity(k), G::CoverWithin(k), G::CoverNever(k)]);
    }
    if data.cover.len() < MOST_COVER_KINDS {
        rows.push(G::CoverAdd);
    }
    for k in 0..data.mobs.len() {
        rows.extend([G::MobPrefab(k), G::MobPerHectare(k), G::MobMost(k), G::MobOn(k), G::MobNear(k), G::MobFar(k)]);
        rows.extend([G::MobWithin(k), G::MobNever(k)]);
    }
    if data.mobs.len() < MOST_MOB_KINDS {
        rows.push(G::MobAdd);
    }
    rows
}

pub fn label(field: GrowthField) -> String {
    match field {
        G::TreeWithin(_) | G::CoverWithin(_) | G::MobWithin(_) => "Within",
        G::TreeNever(_) | G::CoverNever(_) | G::MobNever(_) => "Not Within",
        G::CoverKind(_) => "Cover",
        G::CoverOn(_) | G::MobOn(_) => "On",
        G::CoverSoil(_) => "Soil",
        G::CoverHeight(_) => "Height",
        G::CoverRows(_) => "Rows",
        G::CoverDensity(_) => "Density",
        G::CoverAdd => "Add Cover",
        G::MobPrefab(_) => "Mob",
        G::MobPerHectare(_) => "A Hectare",
        G::MobMost(_) => "Most",
        G::MobNear(_) => "Near",
        G::MobFar(_) => "Far",
        G::MobAdd => "Add Mob",
    }
    .into()
}

/// As `vox editor call field.set` names it, the second and later of a
/// kind numbered after it (`cover_height#2`).
pub fn key(field: GrowthField) -> &'static str {
    match field {
        G::TreeWithin(_) => "tree_within",
        G::TreeNever(_) => "tree_not_within",
        G::CoverKind(_) => "cover",
        G::CoverOn(_) => "cover_on",
        G::CoverSoil(_) => "cover_soil",
        G::CoverHeight(_) => "cover_height",
        G::CoverRows(_) => "cover_rows",
        G::CoverDensity(_) => "cover_density",
        G::CoverWithin(_) => "cover_within",
        G::CoverNever(_) => "cover_not_within",
        G::CoverAdd => "add_cover",
        G::MobPrefab(_) => "mob",
        G::MobPerHectare(_) => "mob_per_hectare",
        G::MobMost(_) => "mob_most",
        G::MobOn(_) => "mob_on",
        G::MobNear(_) => "mob_near",
        G::MobFar(_) => "mob_far",
        G::MobWithin(_) => "mob_within",
        G::MobNever(_) => "mob_not_within",
        G::MobAdd => "add_mob",
    }
}

pub fn tooltip(field: GrowthField) -> &'static str {
    match field {
        G::TreeWithin(_) => "An Area Tag: these trees grow only in World Areas with it. None for anywhere",
        G::TreeNever(_) => "An Area Tag: these trees never grow in World Areas with it",
        G::CoverKind(_) => "What grows, each of its own voxels. None takes it away",
        G::CoverOn(_) => "The ground it grows on",
        G::CoverSoil(_) => "What the ground it grows on is turned to, as a ploughed field's earth",
        G::CoverHeight(_) => "How tall it grows, in metres; for cabbages, how wide a head is",
        G::CoverRows(_) => "How far apart its rows are, in metres; 0 to scatter it",
        G::CoverDensity(_) => "How much of its ground grows a stalk or a head",
        G::CoverWithin(_) => "An Area Tag: it grows only in World Areas with it. None for anywhere",
        G::CoverNever(_) => "An Area Tag: it never grows in World Areas with it",
        G::CoverAdd => "A kind of crop or ground cover to grow",
        G::MobPrefab(_) => "The prefab spawned round the player, a Block Mob to walk. None takes it away",
        G::MobPerHectare(_) => "How many to a hectare round the player",
        G::MobMost(_) => "The most there are at once",
        G::MobOn(_) => "The ground they spawn on",
        G::MobNear(_) => "How near the player they spawn, in metres",
        G::MobFar(_) => "How far from the player they spawn, in metres; they go a fifth again further",
        G::MobWithin(_) => "An Area Tag: they spawn only in World Areas with it. None for anywhere",
        G::MobNever(_) => "An Area Tag: they never spawn in World Areas with it",
        G::MobAdd => "A prefab to spawn on the volume's ground round the player",
    }
}

pub fn unit(field: GrowthField) -> Option<&'static str> {
    matches!(field, G::CoverHeight(_) | G::CoverRows(_) | G::MobNear(_) | G::MobFar(_)).then_some("m")
}

/// Whether the row picks an asset: a tag, or a mob's prefab.
pub fn is_picked(field: GrowthField) -> bool {
    matches!(
        field,
        G::TreeWithin(_)
            | G::TreeNever(_)
            | G::CoverWithin(_)
            | G::CoverNever(_)
            | G::MobPrefab(_)
            | G::MobWithin(_)
            | G::MobNever(_)
            | G::MobAdd
    )
}

pub fn takes_name(field: GrowthField) -> &'static str {
    match field {
        G::MobPrefab(_) | G::MobAdd => "Prefab",
        _ => "Area Tag",
    }
}

/// What a picked row can take, None first.
pub fn choices(game: &Game, types: &ScriptTypes, field: GrowthField) -> Vec<(String, Pick)> {
    match field {
        G::MobPrefab(_) | G::MobAdd => fields::assets_of(game, Some(AssetKind::Prefab)),
        _ => fields::data_assets(game, types, AREA_TAG),
    }
}

fn picked_guid(game: &Game, choice: &Pick) -> Option<Option<AssetGuid>> {
    let Pick::Asset(path) = choice else { return None };
    match path {
        Some(path) => game.content.parse(path).ok().and_then(|reference| reference.guid()).map(Some),
        None => Some(None),
    }
}

fn asset_value(game: &Game, guid: Option<AssetGuid>) -> Value {
    let path = guid.and_then(|guid| game.content.db().get(guid).map(|entry| entry.path.clone()));
    Value::Asset(path.unwrap_or_else(|| "None".into()))
}

fn choice<T: PartialEq + Copy>(all: &[T], one: T, name: fn(T) -> &'static str) -> Value {
    let at = all.iter().position(|other| *other == one).unwrap_or_default();
    Value::Choice(at, all.iter().map(|each| name(*each).to_string()).collect())
}

/// A choice with None first, as for taking a kind away or leaving soil be.
fn or_none<T: PartialEq + Copy>(all: &[T], one: Option<T>, name: fn(T) -> &'static str) -> Value {
    let at = one.and_then(|one| all.iter().position(|other| *other == one)).map_or(0, |at| at + 1);
    let names = std::iter::once("None".to_string()).chain(all.iter().map(|each| name(*each).to_string()));
    Value::Choice(at, names.collect())
}

/// The first tag of a rule, as its row shows it.
fn first(tags: &[AssetGuid]) -> Option<AssetGuid> {
    tags.first().copied()
}

/// A rule's tags with its first set to `tag`, or taken away for none.
fn set_first(tags: &mut Vec<AssetGuid>, tag: Option<AssetGuid>) {
    match (tag, tags.is_empty()) {
        (Some(tag), true) => tags.push(tag),
        (Some(tag), false) => tags[0] = tag,
        (None, false) => drop(tags.remove(0)),
        (None, true) => {}
    }
}

fn ground_of(on: &[VoxelMaterial]) -> VoxelMaterial {
    on.first().copied().unwrap_or(VoxelMaterial::Grass)
}

pub fn get(game: &Game, entity: Entity, field: GrowthField) -> Option<Value> {
    let data = &game.engine.world.get::<VoxelVolumeSource>(entity)?.0;
    let cover = |k: usize| data.cover.get(k);
    let mob = |k: usize| data.mobs.get(k);
    Some(match field {
        G::TreeWithin(k) => asset_value(game, first(&data.trees.get(k)?.within)),
        G::TreeNever(k) => asset_value(game, first(&data.trees.get(k)?.never_within)),
        G::CoverKind(k) => or_none(&VoxelCoverKind::ALL, Some(cover(k)?.kind), VoxelCoverKind::name),
        G::CoverOn(k) => choice(&VoxelMaterial::ALL, ground_of(&cover(k)?.on), VoxelMaterial::name),
        G::CoverSoil(k) => or_none(&VoxelMaterial::ALL, cover(k)?.soil, VoxelMaterial::name),
        G::CoverHeight(k) => Value::Number(cover(k)?.height),
        G::CoverRows(k) => Value::Number(cover(k)?.rows),
        G::CoverDensity(k) => Value::Number(cover(k)?.density),
        G::CoverWithin(k) => asset_value(game, first(&cover(k)?.within)),
        G::CoverNever(k) => asset_value(game, first(&cover(k)?.never_within)),
        G::CoverAdd => or_none(&VoxelCoverKind::ALL, None, VoxelCoverKind::name),
        G::MobPrefab(k) => asset_value(game, mob(k)?.prefab),
        G::MobPerHectare(k) => Value::Number(mob(k)?.per_hectare),
        G::MobMost(k) => Value::Whole(i64::from(mob(k)?.most)),
        G::MobOn(k) => choice(&VoxelMaterial::ALL, ground_of(&mob(k)?.on), VoxelMaterial::name),
        G::MobNear(k) => Value::Number(mob(k)?.near),
        G::MobFar(k) => Value::Number(mob(k)?.far),
        G::MobWithin(k) => asset_value(game, first(&mob(k)?.within)),
        G::MobNever(k) => asset_value(game, first(&mob(k)?.never_within)),
        G::MobAdd => Value::Asset("None".into()),
    })
}

/// Changes the volume's data and makes it again.
fn change(game: &mut Game, entity: Entity, how: impl FnOnce(&mut VoxelVolumeData) -> bool) -> bool {
    let Some(mut data) = game.engine.world.get::<VoxelVolumeSource>(entity).map(|source| source.0.clone()) else {
        return false;
    };
    if !how(&mut data) {
        return false;
    }
    voxear_engine::voxel_volumes::set(&mut game.content, &mut game.engine.world, entity, data);
    true
}

pub fn set(game: &mut Game, entity: Entity, field: GrowthField, value: Value) -> bool {
    change(game, entity, |data| {
        match (field, value) {
            (G::CoverKind(k), Value::Choice(at, _)) if k < data.cover.len() => match at.checked_sub(1) {
                None => drop(data.cover.remove(k)),
                Some(at) => {
                    let Some(&kind) = VoxelCoverKind::ALL.get(at) else { return false };
                    // A kind changed keeps where it grows, and takes the new kind's own shape.
                    let kept = data.cover[k].clone();
                    let (on, within, never_within) = (kept.on, kept.within, kept.never_within);
                    data.cover[k] = VoxelCoverData { on, within, never_within, ..VoxelCoverData::of(kind) };
                }
            },
            (G::CoverAdd, Value::Choice(at, _)) => {
                let Some(&kind) = at.checked_sub(1).and_then(|at| VoxelCoverKind::ALL.get(at)) else { return false };
                data.cover.push(VoxelCoverData::of(kind));
            }
            (G::CoverOn(k), Value::Choice(at, _)) => {
                let (Some(cover), Some(&ground)) = (data.cover.get_mut(k), VoxelMaterial::ALL.get(at)) else {
                    return false;
                };
                set_ground(&mut cover.on, ground);
            }
            (G::CoverSoil(k), Value::Choice(at, _)) => {
                let Some(cover) = data.cover.get_mut(k) else { return false };
                cover.soil = at.checked_sub(1).and_then(|at| VoxelMaterial::ALL.get(at).copied());
            }
            (G::CoverHeight(k), Value::Number(n)) => {
                let Some(cover) = data.cover.get_mut(k) else { return false };
                cover.height = n.clamp(0.05, 8.0);
            }
            (G::CoverRows(k), Value::Number(n)) => {
                let Some(cover) = data.cover.get_mut(k) else { return false };
                cover.rows = n.clamp(0.0, 8.0);
            }
            (G::CoverDensity(k), Value::Number(n)) => {
                let Some(cover) = data.cover.get_mut(k) else { return false };
                cover.density = n.clamp(0.0, 1.0);
            }
            (G::MobPerHectare(k), Value::Number(n)) => {
                let Some(mob) = data.mobs.get_mut(k) else { return false };
                mob.per_hectare = n.clamp(0.0, 400.0);
            }
            (G::MobMost(k), Value::Whole(n)) => {
                let Some(mob) = data.mobs.get_mut(k) else { return false };
                mob.most = n.clamp(0, 256) as u32;
            }
            (G::MobOn(k), Value::Choice(at, _)) => {
                let (Some(mob), Some(&ground)) = (data.mobs.get_mut(k), VoxelMaterial::ALL.get(at)) else {
                    return false;
                };
                set_ground(&mut mob.on, ground);
            }
            (G::MobNear(k), Value::Number(n)) => {
                let Some(mob) = data.mobs.get_mut(k) else { return false };
                mob.near = n.clamp(0.0, 1000.0);
                mob.far = mob.far.max(mob.near);
            }
            (G::MobFar(k), Value::Number(n)) => {
                let Some(mob) = data.mobs.get_mut(k) else { return false };
                mob.far = n.clamp(mob.near, 2000.0);
            }
            _ => return false,
        }
        true
    })
}

/// A list of ground with its first set to `ground`.
fn set_ground(on: &mut Vec<VoxelMaterial>, ground: VoxelMaterial) {
    on.retain(|other| *other != ground);
    on.insert(0, ground);
    on.truncate(1);
}

/// Puts the asset picked in a picked row.
pub fn pick(game: &mut Game, entity: Entity, field: GrowthField, choice: &Pick) -> bool {
    let Some(guid) = picked_guid(game, choice) else { return false };
    if matches!(field, G::MobPrefab(_) | G::MobAdd)
        && guid.is_some_and(|guid| game.content.db().get(guid).map(|entry| entry.kind) != Some(AssetKind::Prefab))
    {
        return false;
    }
    let apply = |data: &mut VoxelVolumeData| -> Option<()> {
        match field {
            G::TreeWithin(k) => set_first(&mut data.trees.get_mut(k)?.within, guid),
            G::TreeNever(k) => set_first(&mut data.trees.get_mut(k)?.never_within, guid),
            G::CoverWithin(k) => set_first(&mut data.cover.get_mut(k)?.within, guid),
            G::CoverNever(k) => set_first(&mut data.cover.get_mut(k)?.never_within, guid),
            G::MobWithin(k) => set_first(&mut data.mobs.get_mut(k)?.within, guid),
            G::MobNever(k) => set_first(&mut data.mobs.get_mut(k)?.never_within, guid),
            G::MobPrefab(k) if k < data.mobs.len() => match guid {
                Some(prefab) => data.mobs[k].prefab = Some(prefab),
                None => drop(data.mobs.remove(k)),
            },
            G::MobAdd => data.mobs.push(VoxelMobsData { prefab: Some(guid?), ..VoxelMobsData::default() }),
            _ => return None,
        }
        Some(())
    };
    change(game, entity, |data| apply(data).is_some())
}
