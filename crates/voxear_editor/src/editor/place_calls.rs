//! The editor's placing calls (`place.*`, see `link`): prefabs made on the
//! ground of a world of blocks, one, lined up along a road or scattered over
//! an area or a brush's circle, as `voxear_engine::prefab_placing` works out
//! where; each call one step to undo.

use serde_json::{Value as Json, json};
use voxear_core::math::{Quat, Vec3};
use voxear_ecs::Entity;
use voxear_engine::{
    local_link::{ErrorKind, LinkError},
    prefab_placing::{self, Candidate, LineUp, Placed, Placement, Side},
    roads::Road,
};
use voxear_render::Renderer;

use super::{
    Editor,
    link_calls::{Called, done},
};
use crate::{
    history::Edit,
    link::{self, bad, need, obj_ref, opt_vector, refuse},
    objs::{self, Destination},
};

/// A prefab to place: its project path, as often as its weight.
struct Choice {
    path: String,
    weight: u32,
}

impl Editor {
    /// The prefabs `args` gives as `prefabs`, each `{ "prefab": <asset>,
    /// "weight": n }` or an asset alone, with each one's Placement.
    fn place_choices(&mut self, args: &Json, renderer: &mut Renderer) -> Result<(Vec<Choice>, Vec<Candidate>), LinkError> {
        let given = need(args, "prefabs")?.as_array().ok_or_else(|| bad("`prefabs` is not a list"))?;
        let (mut choices, mut candidates) = (Vec::new(), Vec::new());
        for entry in given {
            let reference = entry.get("prefab").unwrap_or(entry);
            let path = link::resolve_asset(&self.game, reference)?;
            let weight = entry.get("weight").and_then(Json::as_u64).unwrap_or(1).min(1000) as u32;
            let placement = self.placement_of(&path, renderer).unwrap_or_default();
            choices.push(Choice { path, weight });
            candidates.push(Candidate { weight, placement });
        }
        if choices.is_empty() {
            return Err(bad("`prefabs` is empty"));
        }
        Ok((choices, candidates))
    }

    /// The Placement on a prefab's root, where it has one.
    fn placement_of(&mut self, path: &str, renderer: &mut Renderer) -> Option<Placement> {
        let content = &mut self.game.content;
        let id = content.prefab(Some(&mut *renderer), path).ok()?;
        let template = content.loaded_template(Some(renderer), id).ok()?;
        template.objs.get(template.root()?)?.placement
    }

    /// Makes `choice` at `at` along x and z, turned `yaw`, on the ground
    /// (its Ground Fit's own left out), under `parent`.
    fn place_on_ground(
        &mut self,
        choice: &Choice,
        (at, yaw): ([f32; 2], f32),
        parent: Option<Entity>,
        renderer: &mut Renderer,
    ) -> Result<Entity, LinkError> {
        let game = &mut self.game;
        let guess = Vec3::new(at[0], 0.0, at[1]);
        let made = objs::place_prefab(game, Some(renderer), &choice.path, None, None, guess)
            .map_err(|error| refuse(ErrorKind::Failed, format!("could not place {}: {error:#}", choice.path)))?;
        if let Some(transform) = game.engine.world.get_mut::<voxear_scene::Transform>(made) {
            transform.rotation = Quat::from_rotation_y(yaw);
        }
        voxear_scene::propagate_transforms(&mut game.engine.world);
        let (world, content) = (&mut game.engine.world, &mut game.content);
        let landed = voxear_engine::ground_fit::landing(world, content, made, guess).or_else(|| {
            let ground = voxear_engine::ground_fit::ground_under(world, content, guess, Some(made))?;
            Some(Vec3::new(guess.x, ground, guess.z))
        });
        if let (Some(landed), Some(transform)) = (landed, game.engine.world.get_mut::<voxear_scene::Transform>(made)) {
            transform.position = landed;
        }
        if let Some(parent) = parent {
            objs::move_obj(&mut self.game, made, Destination::Into(parent));
        }
        self.record(Edit::Exists { root: made, taken: None });
        Ok(made)
    }

    /// Makes every one of `placed`, as one step to undo.
    fn place_all(&mut self, choices: &[Choice], placed: &[Placed], parent: Option<Entity>, renderer: &mut Renderer) -> Called {
        let depth = self.history.depth();
        let mut made = Vec::new();
        for spot in placed {
            made.push(self.place_on_ground(&choices[spot.which], (spot.position, spot.yaw), parent, renderer)?);
        }
        self.history.group_since(depth);
        self.shown_again();
        let objs: Vec<Json> = made.iter().map(|&entity| obj_ref(&self.game, entity)).collect();
        done(json!({ "objs": objs }))
    }

    fn parent_arg(&self, args: &Json) -> Result<Option<Entity>, LinkError> {
        args.get("parent").map(|parent| link::resolve_obj(&self.game, parent)).transpose()
    }

    pub(super) fn place_call(&mut self, call: &str, args: &Json, renderer: &mut Renderer) -> Called {
        self.refuse_while_playing("place prefabs")?;
        let (choices, candidates) = self.place_choices(args, renderer)?;
        let parent = self.parent_arg(args)?;
        let seed = args.get("seed").and_then(Json::as_u64).unwrap_or(1);
        let taken = prefab_placing::taken(&self.game.engine.world, &[]);
        let placed = match call {
            "place.prefab" => {
                let at = opt_vector(args, "at")?.ok_or_else(|| bad("`at` is missing"))?;
                let turn = args.get("turn").and_then(Json::as_f64).unwrap_or(0.0) as f32;
                let mut spot = Placed { which: 0, position: [at[0], at[2]], yaw: turn.to_radians() };
                // Facing the road nearest, set back from its edge.
                if args.get("face_road").and_then(Json::as_bool) == Some(true) {
                    let setback = args.get("setback").and_then(Json::as_f64).unwrap_or(1.0) as f32;
                    spot = self.facing_road(spot, &candidates[0].placement, Vec3::from_array(at), setback);
                }
                vec![spot]
            }
            "place.line_up" => {
                let road = link::resolve_obj(&self.game, need(args, "road")?)?;
                let points = self.road_points(road)?;
                let side = match args.get("side").and_then(Json::as_str) {
                    Some("left") => Side::Left,
                    Some("right") => Side::Right,
                    _ => Side::Both,
                };
                let number = |key: &str, default: f32| args.get(key).and_then(Json::as_f64).map_or(default, |v| v as f32);
                let how = LineUp {
                    side,
                    from: number("from", 0.0),
                    to: number("to", 0.0),
                    setback: number("setback", 1.0),
                    jitter: number("jitter", 0.5),
                    gap: number("gap", 1.0),
                    seed,
                };
                prefab_placing::line_up(&points, &candidates, &how, &taken)
            }
            "place.scatter" => {
                let outline = match args.get("area") {
                    Some(area) => self.area_outline(link::resolve_obj(&self.game, area)?)?,
                    None => outline_arg(need(args, "outline")?)?,
                };
                let density = args.get("density").and_then(Json::as_f64).unwrap_or(1.0) as f32;
                prefab_placing::scatter(&outline, &candidates, density, seed, &taken)
            }
            _ => {
                let at = opt_vector(args, "at")?.ok_or_else(|| bad("`at` is missing"))?;
                let radius = args.get("radius").and_then(Json::as_f64).unwrap_or(5.0) as f32;
                if args.get("remove").and_then(Json::as_bool) == Some(true) {
                    return self.brush_away(&choices, Vec3::from_array(at), radius);
                }
                let circle: Vec<[f32; 2]> = (0..24)
                    .map(|i| {
                        let angle = i as f32 / 24.0 * std::f32::consts::TAU;
                        [at[0] + angle.cos() * radius, at[2] + angle.sin() * radius]
                    })
                    .collect();
                let density = args.get("density").and_then(Json::as_f64).unwrap_or(4.0) as f32;
                prefab_placing::scatter(&circle, &candidates, density, seed, &taken)
            }
        };
        self.place_all(&choices, &placed, parent, renderer)
    }

    /// `spot` turned to face the road nearest `at`, set back from its edge.
    fn facing_road(&self, spot: Placed, placement: &Placement, at: Vec3, setback: f32) -> Placed {
        let world = &self.game.engine.world;
        let Some(blocks) = world
            .query_ref::<(Entity, &voxear_engine::blocks::BlockWorldAsset)>()
            .into_iter()
            .find(|(_, asset)| asset.file.generate.as_ref().is_some_and(|generate| generate.endless.is_some()))
            .map(|(entity, _)| entity)
        else {
            return spot;
        };
        let Some((road, middle, _)) = voxear_engine::roads::nearest(world, blocks, at, 60.0, None) else { return spot };
        let half_width = self.road_points(road).ok().map_or(3.0, |points| {
            points.iter().map(|(_, width)| *width).fold(0.0, f32::max) * 0.5
        });
        let away = Vec3::new(at.x - middle.x, 0.0, at.z - middle.z).normalize_or(Vec3::X);
        let out = half_width + setback + placement.size[1] * 0.5;
        let position = [middle.x + away.x * out, middle.z + away.z * out];
        let yaw = (-away.x).atan2(-away.z) - placement.front.to_radians();
        Placed { position, yaw, ..spot }
    }

    /// A road's points along x and z in the world, and its width at each in
    /// metres.
    fn road_points(&self, road: Entity) -> Result<Vec<([f32; 2], f32)>, LinkError> {
        let world = &self.game.engine.world;
        let component = world.get::<Road>(road).ok_or_else(|| refuse(ErrorKind::NoSuchComponent, "the Obj has no Road"))?;
        let size = voxear_engine::roads::world_of(world, road)
            .and_then(|blocks| world.get::<voxear_engine::blocks::BlockWorldAsset>(blocks).map(|a| a.file.block_size))
            .unwrap_or(1.0);
        let placed = voxear_scene::world_matrix(world, road);
        let points = component.world_points(&placed);
        Ok(points.iter().zip(&component.points).map(|(at, point)| ([at.x, at.z], point.width * size)).collect())
    }

    /// An Area Fill's outline along x and z in the world.
    fn area_outline(&self, area: Entity) -> Result<Vec<[f32; 2]>, LinkError> {
        let world = &self.game.engine.world;
        let fill = world.get::<voxear_engine::roads::AreaFill>(area);
        let fill = fill.ok_or_else(|| refuse(ErrorKind::NoSuchComponent, "the Obj has no Area Fill"))?;
        let placed = voxear_scene::world_matrix(world, area);
        Ok(fill.outline.iter().map(|&[x, z]| placed.transform_point3(Vec3::new(x, 0.0, z))).map(|p| [p.x, p.z]).collect())
    }

    /// Takes away the instances of `choices` within `radius` of `at`, as one
    /// step to undo.
    fn brush_away(&mut self, choices: &[Choice], at: Vec3, radius: f32) -> Called {
        let guids: Vec<voxear_assets::AssetGuid> =
            choices.iter().filter_map(|choice| self.game.content.parse(&choice.path).ok()?.guid()).collect();
        let world = &self.game.engine.world;
        let going: Vec<Entity> = world
            .query_ref::<(Entity, &voxear_engine::prefab::PrefabInstance, &voxear_scene::WorldTransform)>()
            .into_iter()
            .filter(|(_, instance, _)| guids.contains(&instance.0))
            .filter(|(_, _, placed)| {
                let p = placed.0.w_axis;
                Vec3::new(p.x - at.x, 0.0, p.z - at.z).length() <= radius
            })
            .map(|(entity, ..)| entity)
            .collect();
        let depth = self.history.depth();
        if self.selected_objs().iter().any(|selected| going.contains(selected)) {
            self.select(None);
        }
        for &entity in &going {
            let taken = objs::take(&mut self.game, entity);
            self.record(Edit::Exists { root: entity, taken: Some(taken) });
        }
        self.history.group_since(depth);
        self.shown_again();
        done(json!({ "removed": going.len() }))
    }
}

fn outline_arg(value: &Json) -> Result<Vec<[f32; 2]>, LinkError> {
    let corners = value.as_array().ok_or_else(|| bad("`outline` is not a list"))?;
    let outline: Option<Vec<[f32; 2]>> = corners
        .iter()
        .map(|corner| {
            let v = corner.as_array()?;
            Some([v.first()?.as_f64()? as f32, v.get(1)?.as_f64()? as f32])
        })
        .collect();
    outline.filter(|outline| outline.len() >= 3).ok_or_else(|| bad("three corners at least, each `[x, z]`"))
}
