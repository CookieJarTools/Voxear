//! Routes: the ways through a world that a game's travellers keep to, a
//! line of points on an Obj of a kind (road, sea, air, or a kind of the
//! game's own, a Route Kind data asset), as wide as it says, one way or both.
//! Every Road is a route of kind road. A Route Stop is a place on a route,
//! with data of the game's own: a harbour, an inn, a waystation.
//!
//! The routes of a kind make a graph, worked out when it is first asked for
//! and again when a route changes: routes meet where their points or ends
//! come within a join's reach of another of the kind, and a stop is joined to
//! the nearest route. [`find`] gives the way from one place to another along
//! them, [`nearest`] the nearest place on one, and [`along`] the place a
//! distance along a way, as a traveller's cursor moves.
//!
//! A sea route is laid by [`lay_sea`] between two places over water deeper
//! than a ship's draught and further than a set distance from the shore,
//! searched over the generator's own ground, so it reaches far beyond the
//! blocks made so far. [`aground`] tells a laid route that a repaint has
//! run onto the land.
//!
//! ```toml
//! [obj.route]
//! points = [[0.0, 0.0, 0.0], [400.0, 0.0, 120.0]]
//! kind = "sea"               # "road", "sea", "air" or "custom", of `custom`
//! width = 12.0
//! one_way = false
//! draught = 3.0              # a sea route's: how deep the water must be
//! shore = 24.0               # and how far from land
//!
//! [obj.route_stop]
//! data = "…"                 # a data asset of the game's own
//! ```

use std::{
    collections::{BinaryHeap, HashMap},
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use voxear_assets::AssetGuid;
use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::{Entity, World};
use voxear_scene::WorldTransform;

/// What travels a route.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RouteKind {
    #[default]
    Road,
    Sea,
    Air,
    /// A kind of the game's own, a Route Kind data asset.
    Custom,
}

impl RouteKind {
    pub const ALL: [RouteKind; 4] = [RouteKind::Road, RouteKind::Sea, RouteKind::Air, RouteKind::Custom];

    pub fn name(self) -> &'static str {
        match self {
            RouteKind::Road => "Road",
            RouteKind::Sea => "Sea",
            RouteKind::Air => "Air",
            RouteKind::Custom => "Custom",
        }
    }
}

/// A kind of route asked for: one of the built-in kinds, or a Route Kind
/// asset of the game's own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct KindOf {
    pub kind: RouteKind,
    pub custom: Option<AssetGuid>,
}

impl KindOf {
    pub fn of(kind: RouteKind) -> Self {
        Self { kind, custom: None }
    }
}

/// A route: a line of points in its Obj's own space.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Route {
    pub points: Vec<[f32; 3]>,
    pub kind: RouteKind,
    /// The Route Kind data asset of a custom route.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom: Option<AssetGuid>,
    /// How wide it is, in metres.
    pub width: f32,
    /// Whether it is travelled only from its first point towards its last.
    pub one_way: bool,
    /// A sea route's: the least depth of water it keeps to, in metres, and
    /// the least distance from land.
    pub draught: f32,
    pub shore: f32,
    /// Whether the game's maps draw it.
    pub on_map: bool,
}

impl voxear_ecs::Component for Route {}

impl Default for Route {
    fn default() -> Self {
        Self {
            points: vec![[0.0; 3], [0.0, 0.0, 40.0]],
            kind: RouteKind::Road,
            custom: None,
            width: 6.0,
            one_way: false,
            draught: 3.0,
            shore: 24.0,
            on_map: true,
        }
    }
}

impl Route {
    pub fn kind_of(&self) -> KindOf {
        KindOf { kind: self.kind, custom: self.custom.filter(|_| self.kind == RouteKind::Custom) }
    }

    pub fn world_points(&self, placed: &Mat4) -> Vec<Vec3> {
        self.points.iter().map(|&at| placed.transform_point3(Vec3::from(at))).collect()
    }
}

/// A place on a route, with data of the game's own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RouteStop {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<AssetGuid>,
}

impl voxear_ecs::Component for RouteStop {}

/// How near another's point or line a route's point or end joins it, in
/// metres, past half their widths.
const JOIN: f32 = 2.0;
/// How far from its stop a route may run and still be joined to it.
const STOP_REACH: f32 = 24.0;

/// One route of a graph, in the world.
#[derive(Clone, Debug, PartialEq)]
struct Line {
    obj: Entity,
    points: Vec<Vec3>,
    width: f32,
    one_way: bool,
}

/// The routes of one kind, joined.
#[derive(Debug, Default)]
pub struct Graph {
    lines: Vec<Line>,
    nodes: Vec<Vec3>,
    /// Each node's ways on, and what each costs, in metres.
    ways: Vec<Vec<(usize, f32)>>,
    /// The node each stop is joined at, with its Obj.
    stops: Vec<(Entity, usize)>,
    /// The nodes put on each segment, by its first node, with how far along
    /// it each lies, from 0 to 1.
    put_on: HashMap<usize, Vec<(f32, usize)>>,
}

/// The graphs worked out so far, by kind, each with what it was made from.
#[derive(Default)]
struct Graphs(HashMap<KindOf, (u64, Arc<Graph>)>);

fn lines_of(world: &World, kind: KindOf) -> Vec<Line> {
    let here = crate::dimensions::current(world);
    let shown = |obj: Entity| voxear_scene::is_active(world, obj) && voxear_scene::dimension_of(world, obj) == here;
    let routes = world.query_ref::<(Entity, &Route, &WorldTransform)>();
    let mut lines: Vec<Line> = routes
        .without::<voxear_scene::Destroying>()
        .into_iter()
        .filter(|(obj, route, _)| shown(*obj) && route.kind_of() == kind && route.points.len() >= 2)
        .map(|(obj, route, placed)| Line {
            obj,
            points: route.world_points(&placed.0),
            width: route.width,
            one_way: route.one_way,
        })
        .collect();
    // Every Road is a route of kind road.
    if kind == KindOf::of(RouteKind::Road) {
        let roads = world.query_ref::<(Entity, &crate::roads::Road, &WorldTransform)>();
        let roads = roads.without::<voxear_scene::Destroying>().into_iter().filter(|(obj, ..)| shown(*obj));
        lines.extend(roads.filter(|(_, road, _)| road.points.len() >= 2).map(|(obj, road, placed)| {
            let size = crate::roads::world_of(world, obj)
                .and_then(|blocks| world.get::<crate::blocks::BlockWorldAsset>(blocks).map(|a| a.file.block_size))
                .unwrap_or(1.0);
            let width = road.points.iter().map(|point| point.width).fold(0.0, f32::max) * size;
            Line { obj, points: road.world_points(&placed.0), width, one_way: false }
        }));
    }
    lines.sort_by_key(|line| line.obj.to_bits());
    lines
}

fn stops_of(world: &World) -> Vec<(Entity, Vec3)> {
    let here = crate::dimensions::current(world);
    let stops = world.query_ref::<(Entity, &RouteStop, &WorldTransform)>();
    let mut found: Vec<(Entity, Vec3)> = stops
        .without::<voxear_scene::Destroying>()
        .into_iter()
        .filter(|(obj, ..)| voxear_scene::is_active(world, *obj) && voxear_scene::dimension_of(world, *obj) == here)
        .map(|(obj, _, placed)| (obj, placed.0.w_axis.truncate()))
        .collect();
    found.sort_by_key(|(obj, _)| obj.to_bits());
    found
}

/// The graph of the routes of kind `kind`, kept while none of them changes.
pub fn graph(world: &mut World, kind: KindOf) -> Arc<Graph> {
    let lines = lines_of(world, kind);
    let stops = stops_of(world);
    let mut bytes = Vec::new();
    for line in &lines {
        bytes.extend(line.obj.to_bits().to_le_bytes());
        line.points.iter().for_each(|at| at.to_array().iter().for_each(|v| bytes.extend(v.to_le_bytes())));
        bytes.extend(line.width.to_le_bytes());
        bytes.push(u8::from(line.one_way));
    }
    for (obj, at) in &stops {
        bytes.extend(obj.to_bits().to_le_bytes());
        at.to_array().iter().for_each(|v| bytes.extend(v.to_le_bytes()));
    }
    let made_from = voxear_terrain::layers::stable_hash(&bytes);
    if let Some((_, kept)) =
        world.resource::<Graphs>().and_then(|graphs| graphs.0.get(&kind)).filter(|(was, _)| *was == made_from)
    {
        return kept.clone();
    }
    let made = Arc::new(Graph::new(lines, &stops));
    if world.resource::<Graphs>().is_none() {
        world.insert_resource(Graphs::default());
    }
    world.resource_mut::<Graphs>().expect("put in above").0.insert(kind, (made_from, made.clone()));
    made
}

/// The nearest point to `at` on the segment from `a` to `b`, and how far
/// along it that is, from 0 to 1.
fn on_segment(at: Vec3, a: Vec3, b: Vec3) -> (Vec3, f32) {
    let ab = b - a;
    let length = ab.length_squared();
    let along = if length > 0.0 { ((at - a).dot(ab) / length).clamp(0.0, 1.0) } else { 0.0 };
    (a + ab * along, along)
}

impl Graph {
    fn new(lines: Vec<Line>, stops: &[(Entity, Vec3)]) -> Self {
        let mut graph = Graph { lines, ..Graph::default() };
        // Each line's points, joined in turn.
        let mut first_of = Vec::new();
        for line in 0..graph.lines.len() {
            let first = graph.nodes.len();
            first_of.push(first);
            let (points, one_way) = (graph.lines[line].points.clone(), graph.lines[line].one_way);
            for (at, point) in points.iter().enumerate() {
                let node = graph.node(*point);
                if at > 0 {
                    graph.way(node - 1, node, !one_way);
                }
            }
        }
        // Lines meet where a point of one comes near another's, or an end near another's line.
        for line in 0..graph.lines.len() {
            for other in 0..graph.lines.len() {
                if other == line {
                    continue;
                }
                let reach = (graph.lines[line].width + graph.lines[other].width) * 0.5 + JOIN;
                let count = graph.lines[line].points.len();
                for at in 0..count {
                    let node = first_of[line] + at;
                    let point = graph.nodes[node];
                    let near = (0..graph.lines[other].points.len())
                        .map(|o| (first_of[other] + o, graph.nodes[first_of[other] + o]));
                    let near: Vec<usize> =
                        near.filter(|(_, there)| there.distance(point) <= reach).map(|(n, _)| n).collect();
                    for there in &near {
                        graph.way(node, *there, true);
                    }
                    let end = at == 0 || at + 1 == count;
                    if end && near.is_empty() {
                        if let Some((on, joined)) = graph.join_onto(other, first_of[other], point, reach) {
                            graph.way(node, joined, true);
                            let _ = on;
                        }
                    }
                }
            }
        }
        // Each stop joins the nearest line within its reach.
        for &(obj, at) in stops {
            let nearest = (0..graph.lines.len())
                .filter_map(|line| {
                    let reach = graph.lines[line].width * 0.5 + STOP_REACH;
                    graph
                        .nearest_on(line, first_of[line], at)
                        .filter(|(point, ..)| point.distance(at) <= reach)
                        .map(|found| (line, found))
                })
                .min_by(|a, b| a.1.0.distance(at).total_cmp(&b.1.0.distance(at)));
            if let Some((line, _)) = nearest {
                if let Some((_, joined)) = graph.join_onto(line, first_of[line], at, f32::MAX) {
                    graph.stops.push((obj, joined));
                }
            }
        }
        graph
    }

    fn node(&mut self, at: Vec3) -> usize {
        self.nodes.push(at);
        self.ways.push(Vec::new());
        self.nodes.len() - 1
    }

    fn way(&mut self, from: usize, to: usize, both: bool) {
        let cost = self.nodes[from].distance(self.nodes[to]);
        self.ways[from].push((to, cost));
        if both {
            self.ways[to].push((from, cost));
        }
    }

    /// The nearest point to `at` on line `line`, whose first node is `first`:
    /// the point, and the segment it lies on, by its first point.
    fn nearest_on(&self, line: usize, first: usize, at: Vec3) -> Option<(Vec3, usize, f32)> {
        let points = &self.lines[line].points;
        (1..points.len())
            .map(|b| {
                let (point, along) = on_segment(at, points[b - 1], points[b]);
                (point, first + b - 1, along)
            })
            .min_by(|a, b| a.0.distance(at).total_cmp(&b.0.distance(at)))
    }

    /// A node put on line `line` at its nearest point to `at`, within
    /// `reach`, joined to the segment's two ends as the line runs.
    fn join_onto(&mut self, line: usize, first: usize, at: Vec3, reach: f32) -> Option<(Vec3, usize)> {
        let (point, a, along) = self.nearest_on(line, first, at)?;
        if point.distance(at) > reach {
            return None;
        }
        let one_way = self.lines[line].one_way;
        let node = self.node(point);
        self.way(a, node, !one_way);
        self.way(node, a + 1, !one_way);
        // And to those put on the segment before it, in the order they lie along it.
        let others = self.put_on.get(&a).cloned().unwrap_or_default();
        for (other_along, other) in others {
            match other_along <= along {
                true => self.way(other, node, !one_way),
                false => self.way(node, other, !one_way),
            }
        }
        self.put_on.entry(a).or_default().push((along, node));
        Some((point, node))
    }

    /// The nearest place to `at` on any of its routes: the route's Obj, the
    /// place, and how far along the route it lies, in metres.
    pub fn nearest(&self, at: Vec3) -> Option<(Entity, Vec3, f32)> {
        let mut best: Option<(f32, Entity, Vec3, f32)> = None;
        for line in &self.lines {
            let mut gone = 0.0;
            for b in 1..line.points.len() {
                let (a, b) = (line.points[b - 1], line.points[b]);
                let (point, along) = on_segment(at, a, b);
                let distance = point.distance(at);
                if best.is_none_or(|(was, ..)| distance < was) {
                    best = Some((distance, line.obj, point, gone + a.distance(b) * along));
                }
                gone += a.distance(b);
            }
        }
        best.map(|(_, obj, point, along)| (obj, point, along))
    }

    /// The way from `from` to `to` along its routes: from the nearest place
    /// on one to `from` to the nearest to `to`, its points, and the stops it
    /// passes, in turn. None where no way joins them.
    pub fn find(&self, from: Vec3, to: Vec3) -> Option<Way> {
        let mut graph = Graph {
            lines: self.lines.clone(),
            nodes: self.nodes.clone(),
            ways: self.ways.clone(),
            stops: self.stops.clone(),
            put_on: self.put_on.clone(),
        };
        let mut firsts = Vec::new();
        let mut count = 0;
        for line in &graph.lines {
            firsts.push(count);
            count += line.points.len();
        }
        let ends = [from, to].map(|at| {
            let nearest = (0..graph.lines.len())
                .filter_map(|line| graph.nearest_on(line, firsts[line], at).map(|found| (line, found.0)))
                .min_by(|a, b| a.1.distance(at).total_cmp(&b.1.distance(at)));
            nearest.and_then(|(line, _)| graph.join_onto(line, firsts[line], at, f32::MAX)).map(|(_, node)| node)
        });
        let [Some(start), Some(goal)] = ends else { return None };
        let path = graph.shortest(start, goal)?;
        let points: Vec<Vec3> = path.iter().map(|&node| graph.nodes[node]).collect();
        // The stops it passes, in the order it passes them: those joined at a place on its way.
        let mut stops: Vec<(f32, Entity)> = Vec::new();
        for &(obj, node) in &graph.stops {
            let at = graph.nodes[node];
            let mut gone = 0.0;
            for pair in points.windows(2) {
                let (on, along) = on_segment(at, pair[0], pair[1]);
                if on.distance(at) < 0.05 {
                    stops.push((gone + pair[0].distance(pair[1]) * along, obj));
                    break;
                }
                gone += pair[0].distance(pair[1]);
            }
        }
        stops.sort_by(|a, b| a.0.total_cmp(&b.0));
        let stops: Vec<Entity> = stops.into_iter().map(|(_, obj)| obj).collect();
        let length = points.windows(2).map(|pair| pair[0].distance(pair[1])).sum();
        let mut points = points;
        points.dedup_by(|a, b| a.distance(*b) < 1e-3);
        Some(Way { points, length, stops })
    }

    /// The nodes of the shortest way from `start` to `goal`.
    fn shortest(&self, start: usize, goal: usize) -> Option<Vec<usize>> {
        #[derive(PartialEq)]
        struct Open(f32, usize);
        impl Eq for Open {}
        impl PartialOrd for Open {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }
        impl Ord for Open {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                other.0.total_cmp(&self.0).then(other.1.cmp(&self.1))
            }
        }
        let goal_at = self.nodes[goal];
        let mut cost = vec![f32::MAX; self.nodes.len()];
        let mut came = vec![usize::MAX; self.nodes.len()];
        let mut open = BinaryHeap::new();
        cost[start] = 0.0;
        open.push(Open(self.nodes[start].distance(goal_at), start));
        while let Some(Open(_, node)) = open.pop() {
            if node == goal {
                let mut path = vec![goal];
                while let Some(&last) = path.last().filter(|&&last| last != start) {
                    path.push(came[last]);
                }
                path.reverse();
                return Some(path);
            }
            for &(next, step) in &self.ways[node] {
                let through = cost[node] + step;
                if through < cost[next] {
                    cost[next] = through;
                    came[next] = node;
                    open.push(Open(through + self.nodes[next].distance(goal_at), next));
                }
            }
        }
        None
    }

    /// The stops it joins, each with where it is joined.
    pub fn stops(&self) -> impl Iterator<Item = (Entity, Vec3)> + '_ {
        self.stops.iter().map(|&(obj, node)| (obj, self.nodes[node]))
    }
}

/// A way along routes.
#[derive(Clone, Debug, PartialEq)]
pub struct Way {
    pub points: Vec<Vec3>,
    pub length: f32,
    pub stops: Vec<Entity>,
}

/// The way from `from` to `to` along the routes of kind `kind`.
pub fn find(world: &mut World, from: Vec3, to: Vec3, kind: KindOf) -> Option<Way> {
    graph(world, kind).find(from, to)
}

/// The nearest place to `at` on a route of kind `kind`: the route's Obj, the
/// place, and how far along the route it lies, in metres.
pub fn nearest(world: &mut World, at: Vec3, kind: KindOf) -> Option<(Entity, Vec3, f32)> {
    graph(world, kind).nearest(at)
}

/// The place `distance` metres along the line `points`, and the way it runs
/// there: the start before it, the end past it.
pub fn along(points: &[Vec3], distance: f32) -> Option<(Vec3, Vec3)> {
    let first = *points.first()?;
    let mut left = distance.max(0.0);
    for pair in points.windows(2) {
        let length = pair[0].distance(pair[1]);
        if left <= length && length > 0.0 {
            return Some((pair[0].lerp(pair[1], left / length), (pair[1] - pair[0]) / length));
        }
        left -= length;
    }
    let heading = points.windows(2).last().map_or(Vec3::Z, |pair| (pair[1] - pair[0]).normalize_or_zero());
    Some((*points.last().unwrap_or(&first), heading))
}

/// The ground the generator of the world of blocks on `blocks` makes, as a
/// sampler of heights in the world's metres, and the sea's height.
struct Seabed {
    ground: Box<dyn Fn(f32, f32) -> f32>,
    sea: f32,
}

fn seabed(world: &World, content: &mut crate::content::Content, blocks: Entity) -> Option<Seabed> {
    let asset = world.get::<crate::blocks::BlockWorldAsset>(blocks)?.clone();
    let making = crate::blocks::making(content, &asset)?;
    let corner = crate::ground_fit::world_corner(world, blocks)?;
    let size = asset.file.block_size.max(0.01);
    let generate = making.generate.clone();
    let ground = voxear_terrain::blocks::generate::ground_generator(&generate, &making.world, size);
    let sea = corner.y + making.world.sea_level();
    let position = generate.position;
    Some(Seabed {
        ground: Box::new(move |x, z| {
            corner.y + ground.height_at(position[0] + x - corner.x, position[1] + z - corner.z)
        }),
        sea,
    })
}

/// The world of blocks without end of the dimension the game is seen from.
fn endless(world: &World) -> Option<Entity> {
    let here = crate::dimensions::current(world);
    world
        .query_ref::<(Entity, &crate::blocks::BlockWorldAsset)>()
        .without::<voxear_scene::Destroying>()
        .into_iter()
        .filter(|(_, asset)| asset.file.generate.as_ref().is_some_and(|generate| generate.endless.is_some()))
        .map(|(entity, _)| entity)
        .find(|entity| voxear_scene::dimension_of(world, *entity) == here)
}

/// How far apart the sea search's cells are, at the least, in metres.
const SEA_CELL: f32 = 16.0;
/// The most cells along a side the sea search looks over.
const SEA_CELLS: usize = 384;

/// A way over the sea from `from` to `to`, at the sea's height, over water
/// at least `draught` metres deep and at least `shore` metres from land,
/// searched over the generator's ground of the world without end; its ends
/// joined straight to the nearest such water. None where the sea does not
/// join them within the search.
pub fn lay_sea(
    world: &World,
    content: &mut crate::content::Content,
    from: Vec3,
    to: Vec3,
    draught: f32,
    shore: f32,
) -> Option<Vec<Vec3>> {
    let bed = seabed(world, content, endless(world)?)?;
    let span = from.distance(to);
    let margin = (span * 0.5).max(256.0);
    let low = [from.x.min(to.x) - margin, from.z.min(to.z) - margin];
    let high = [from.x.max(to.x) + margin, from.z.max(to.z) + margin];
    let cell = ((high[0] - low[0]).max(high[1] - low[1]) / SEA_CELLS as f32).max(SEA_CELL);
    let side = [((high[0] - low[0]) / cell).ceil() as usize + 1, ((high[1] - low[1]) / cell).ceil() as usize + 1];
    let middle = |[x, z]: [usize; 2]| [low[0] + x as f32 * cell, low[1] + z as f32 * cell];
    let index = |[x, z]: [usize; 2]| z * side[0] + x;
    // How deep the water is over each cell, and how far each is from land, in cells.
    let depth: Vec<f32> = (0..side[1])
        .flat_map(|z| (0..side[0]).map(move |x| [x, z]))
        .map(|at| {
            let [x, z] = middle(at);
            bed.sea - (bed.ground)(x, z)
        })
        .collect();
    let from_land = distance_from_land(&depth, side);
    let clear = |at: [usize; 2]| {
        let i = index(at);
        depth[i] >= draught && from_land[i] * cell >= shore
    };
    let cell_of = |at: Vec3| {
        [
            ((at.x - low[0]) / cell).round().clamp(0.0, (side[0] - 1) as f32) as usize,
            ((at.z - low[1]) / cell).round().clamp(0.0, (side[1] - 1) as f32) as usize,
        ]
    };
    // Each end starts from the nearest clear water.
    let nearest_clear = |at: [usize; 2]| -> Option<[usize; 2]> {
        let reach = side[0].max(side[1]);
        (0..reach).find_map(|ring| {
            let ring = ring as i64;
            let mut found: Option<([usize; 2], i64)> = None;
            for dz in -ring..=ring {
                for dx in -ring..=ring {
                    if dx.abs() != ring && dz.abs() != ring {
                        continue;
                    }
                    let (x, z) = (at[0] as i64 + dx, at[1] as i64 + dz);
                    if x < 0 || z < 0 || x >= side[0] as i64 || z >= side[1] as i64 {
                        continue;
                    }
                    let near = [x as usize, z as usize];
                    let far = dx * dx + dz * dz;
                    if clear(near) && found.is_none_or(|(_, was)| far < was) {
                        found = Some((near, far));
                    }
                }
            }
            found.map(|(near, _)| near)
        })
    };
    let start = nearest_clear(cell_of(from))?;
    let goal = nearest_clear(cell_of(to))?;
    let path = search(side, start, goal, &clear)?;
    // Straightened: each point on to the furthest it sees over clear water.
    let sees = |a: [usize; 2], b: [usize; 2]| {
        let steps = (a[0].abs_diff(b[0]).max(a[1].abs_diff(b[1])) * 2).max(1);
        (0..=steps).all(|step| {
            let t = step as f32 / steps as f32;
            let x = a[0] as f32 + (b[0] as f32 - a[0] as f32) * t;
            let z = a[1] as f32 + (b[1] as f32 - a[1] as f32) * t;
            clear([x.round() as usize, z.round() as usize])
        })
    };
    let mut kept = vec![path[0]];
    let mut at = 0;
    while at + 1 < path.len() {
        let next = (at + 1..path.len()).rev().find(|&next| sees(path[at], path[next])).unwrap_or(at + 1);
        kept.push(path[next]);
        at = next;
    }
    let mut points = vec![Vec3::new(from.x, bed.sea, from.z)];
    points.extend(kept.into_iter().map(|at| {
        let [x, z] = middle(at);
        Vec3::new(x, bed.sea, z)
    }));
    points.push(Vec3::new(to.x, bed.sea, to.z));
    points.dedup_by(|a, b| a.distance(*b) < 1.0);
    Some(points)
}

/// How far each cell lies from the nearest whose ground is over the sea, in
/// cells, by eight neighbours.
fn distance_from_land(depth: &[f32], side: [usize; 2]) -> Vec<f32> {
    let mut far = vec![f32::MAX; depth.len()];
    let mut open = BinaryHeap::new();
    for (at, &deep) in depth.iter().enumerate() {
        if deep <= 0.0 {
            far[at] = 0.0;
            open.push(std::cmp::Reverse((0u32, at)));
        }
    }
    while let Some(std::cmp::Reverse((_, at))) = open.pop() {
        let (x, z) = ((at % side[0]) as i64, (at / side[0]) as i64);
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
            let (nx, nz) = (x + dx, z + dz);
            if nx < 0 || nz < 0 || nx >= side[0] as i64 || nz >= side[1] as i64 {
                continue;
            }
            let next = nz as usize * side[0] + nx as usize;
            let step = if dx != 0 && dz != 0 { std::f32::consts::SQRT_2 } else { 1.0 };
            if far[at] + step < far[next] {
                far[next] = far[at] + step;
                open.push(std::cmp::Reverse(((far[next] * 64.0) as u32, next)));
            }
        }
    }
    far
}

/// The cells of the shortest way from `start` to `goal` over clear cells,
/// by eight neighbours.
fn search(
    side: [usize; 2],
    start: [usize; 2],
    goal: [usize; 2],
    clear: &dyn Fn([usize; 2]) -> bool,
) -> Option<Vec<[usize; 2]>> {
    let index = |[x, z]: [usize; 2]| z * side[0] + x;
    let guess = |[x, z]: [usize; 2]| {
        let (dx, dz) = (x.abs_diff(goal[0]) as f32, z.abs_diff(goal[1]) as f32);
        (dx * dx + dz * dz).sqrt()
    };
    let mut cost = vec![f32::MAX; side[0] * side[1]];
    let mut came = vec![usize::MAX; side[0] * side[1]];
    let mut open = BinaryHeap::new();
    cost[index(start)] = 0.0;
    open.push(std::cmp::Reverse(((guess(start) * 64.0) as u32, index(start))));
    while let Some(std::cmp::Reverse((_, at))) = open.pop() {
        let here = [at % side[0], at / side[0]];
        if here == goal {
            let mut path = vec![goal];
            let mut back = at;
            while back != index(start) {
                back = came[back];
                path.push([back % side[0], back / side[0]]);
            }
            path.reverse();
            return Some(path);
        }
        for (dx, dz) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
            let (nx, nz) = (here[0] as i64 + dx, here[1] as i64 + dz);
            if nx < 0 || nz < 0 || nx >= side[0] as i64 || nz >= side[1] as i64 {
                continue;
            }
            let next = [nx as usize, nz as usize];
            if !clear(next) {
                continue;
            }
            let step = if dx != 0 && dz != 0 { std::f32::consts::SQRT_2 } else { 1.0 };
            let through = cost[at] + step;
            if through < cost[index(next)] {
                cost[index(next)] = through;
                came[index(next)] = at;
                open.push(std::cmp::Reverse((((through + guess(next)) * 64.0) as u32, index(next))));
            }
        }
    }
    None
}

/// What was last found of each sea route running aground, with what it was
/// found for.
#[derive(Default)]
struct Aground(HashMap<Entity, (u64, bool)>);

/// Whether the sea route on `obj` runs over water shallower than its
/// draught anywhere between its ends, as the generator's ground now is: so
/// after a repaint has raised land under it. Kept while the route and the
/// world's painted maps stay as they are.
pub fn aground(world: &mut World, content: &mut crate::content::Content, obj: Entity) -> bool {
    let Some(route) = world.get::<Route>(obj).filter(|route| route.kind == RouteKind::Sea).map(|r| r.clone()) else {
        return false;
    };
    let placed = world.get::<WorldTransform>(obj).map_or(Mat4::IDENTITY, |placed| placed.0);
    let Some(blocks) = endless(world) else { return false };
    let painted =
        world.get::<crate::blocks::BlockWorldAsset>(blocks).and_then(|asset| content.painted(asset.guid, false));
    let mut bytes: Vec<u8> = route.points.iter().flatten().flat_map(|v| v.to_le_bytes()).collect();
    placed.to_cols_array().iter().for_each(|v| bytes.extend(v.to_le_bytes()));
    bytes.extend(route.draught.to_le_bytes());
    bytes.extend(painted.map_or(0, |painted| painted.maps().version).to_le_bytes());
    let made_from = voxear_terrain::layers::stable_hash(&bytes);
    if let Some(&(_, found)) =
        world.resource::<Aground>().and_then(|kept| kept.0.get(&obj)).filter(|(was, _)| *was == made_from)
    {
        return found;
    }
    let found = seabed(world, content, blocks).is_some_and(|bed| {
        let points = route.world_points(&placed);
        // Its ends are joined to the shore, so only what lies between them is asked.
        let length: f32 = points.windows(2).map(|pair| pair[0].distance(pair[1])).sum();
        let ends = (route.shore + SEA_CELL * 2.0).min(length * 0.25);
        let steps = (length / 8.0).ceil() as usize;
        (0..=steps).any(|step| {
            let gone = step as f32 * 8.0;
            if gone < ends || gone > length - ends {
                return false;
            }
            along(&points, gone).is_some_and(|(at, _)| bed.sea - (bed.ground)(at.x, at.z) < route.draught)
        })
    });
    if world.resource::<Aground>().is_none() {
        world.insert_resource(Aground::default());
    }
    world.resource_mut::<Aground>().expect("put in above").0.insert(obj, (made_from, found));
    found
}

/// Each route the maps draw, as its points in the world and its colour, of
/// the dimension `here`.
pub fn on_maps(world: &World, here: voxear_scene::DimensionId) -> Vec<(Vec<Vec3>, [u8; 3])> {
    let routes = world.query_ref::<(Entity, &Route, &WorldTransform)>();
    let routes = routes.without::<voxear_scene::Destroying>().into_iter();
    routes
        .filter(|(obj, route, _)| {
            route.on_map && voxear_scene::is_active(world, *obj) && voxear_scene::dimension_of(world, *obj) == here
        })
        .map(|(_, route, placed)| (route.world_points(&placed.0), kind_color(route.kind)))
        .collect()
}

/// The colour a kind of route is drawn in.
pub fn kind_color(kind: RouteKind) -> [u8; 3] {
    match kind {
        RouteKind::Road => [242, 178, 76],
        RouteKind::Sea => [90, 178, 255],
        RouteKind::Air => [230, 242, 255],
        RouteKind::Custom => [190, 128, 255],
    }
}

/// The kind a script asks for: `"road"`, `"sea"`, `"air"`, or a Route Kind
/// asset's GUID.
fn kind_asked(asked: &serde_json::Value) -> KindOf {
    match asked["kind"].as_str() {
        Some("sea") => KindOf::of(RouteKind::Sea),
        Some("air") => KindOf::of(RouteKind::Air),
        Some(guid) if guid.parse::<AssetGuid>().is_ok() => {
            KindOf { kind: RouteKind::Custom, custom: guid.parse::<AssetGuid>().ok() }
        }
        _ => KindOf::of(RouteKind::Road),
    }
}

fn point_asked(asked: &serde_json::Value, key: &str) -> anyhow::Result<Vec3> {
    let at = asked[key].as_array().map(|v| v.iter().filter_map(serde_json::Value::as_f64).collect::<Vec<f64>>());
    let Some([x, y, z]) = at.and_then(|at| <[f64; 3]>::try_from(at).ok()) else {
        anyhow::bail!("`{key}` is not a place")
    };
    Ok(Vec3::new(x as f32, y as f32, z as f32))
}

/// Answers a script's question about the world's routes, as JSON: `{ "ask":
/// "route_find", "from": [x, y, z], "to", "kind" }` gives `{ "points",
/// "length", "stops" }` or nothing; `{ "ask": "route_nearest", "at", "kind" }`
/// gives `{ "obj", "point", "along" }` or nothing.
pub fn answer(world: &mut World, asked: &serde_json::Value) -> anyhow::Result<String> {
    let kind = kind_asked(asked);
    match asked["ask"].as_str() {
        Some("route_find") => {
            let (from, to) = (point_asked(asked, "from")?, point_asked(asked, "to")?);
            Ok(find(world, from, to, kind).map_or_else(String::new, |way| way_json(&way).to_string()))
        }
        Some("route_nearest") => {
            let at = point_asked(asked, "at")?;
            Ok(nearest(world, at, kind).map_or_else(String::new, |(obj, point, along)| {
                serde_json::json!({ "obj": obj.to_bits(), "point": point.to_array(), "along": along }).to_string()
            }))
        }
        other => anyhow::bail!("no such question about the world's routes: {other:?}"),
    }
}

/// Lays a sea route, as a script asks: `{ "ask": "route_lay_sea", "from",
/// "to", "draught", "shore" }` gives the points, or nothing where the sea
/// does not join them.
pub fn answer_lay(
    world: &World,
    content: &mut crate::content::Content,
    asked: &serde_json::Value,
) -> anyhow::Result<String> {
    let (from, to) = (point_asked(asked, "from")?, point_asked(asked, "to")?);
    let draught = asked["draught"].as_f64().unwrap_or(3.0) as f32;
    let shore = asked["shore"].as_f64().unwrap_or(24.0) as f32;
    let laid = lay_sea(world, content, from, to, draught, shore);
    Ok(laid.map_or_else(String::new, |points| {
        serde_json::json!(points.iter().map(|at| at.to_array()).collect::<Vec<_>>()).to_string()
    }))
}

/// A way as JSON: its points, its length and the Objs of its stops.
pub fn way_json(way: &Way) -> serde_json::Value {
    serde_json::json!({
        "points": way.points.iter().map(|at| at.to_array()).collect::<Vec<_>>(),
        "length": way.length,
        "stops": way.stops.iter().map(|obj| obj.to_bits()).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(bits: u32, points: &[[f32; 3]], one_way: bool) -> Line {
        Line {
            obj: Entity::from_bits(u64::from(bits) | (1 << 32)),
            points: points.iter().map(|&at| Vec3::from(at)).collect(),
            width: 4.0,
            one_way,
        }
    }

    #[test]
    fn a_way_is_found_along_routes_that_meet_and_passes_their_stops() {
        // An east road, a north road whose end meets the east one's middle.
        let lines = vec![
            line(1, &[[0.0, 0.0, 0.0], [100.0, 0.0, 0.0]], false),
            line(2, &[[50.0, 0.0, 2.0], [50.0, 0.0, 100.0]], false),
        ];
        let stop = Entity::from_bits(9 | (1 << 32));
        let graph = Graph::new(lines, &[(stop, Vec3::new(50.0, 0.0, 60.0))]);
        let way = graph.find(Vec3::new(-5.0, 0.0, 0.0), Vec3::new(50.0, 0.0, 105.0)).expect("a way");
        assert!((way.length - 150.0).abs() < 3.0, "{}", way.length);
        assert_eq!(way.stops, [stop]);
        let (obj, point, along) = graph.nearest(Vec3::new(30.0, 0.0, 10.0)).expect("a route");
        assert_eq!(obj.to_bits() as u32, 1);
        assert!(point.distance(Vec3::new(30.0, 0.0, 0.0)) < 1e-3);
        assert!((along - 30.0).abs() < 1e-3);
    }

    #[test]
    fn a_one_way_route_is_found_only_the_way_it_runs() {
        let graph = Graph::new(vec![line(1, &[[0.0, 0.0, 0.0], [100.0, 0.0, 0.0]], true)], &[]);
        assert!(graph.find(Vec3::new(0.0, 0.0, 0.0), Vec3::new(100.0, 0.0, 0.0)).is_some());
        assert!(graph.find(Vec3::new(100.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 0.0)).is_none());
    }

    #[test]
    fn a_cursor_moves_by_distance_along_a_way() {
        let points = [Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 10.0)];
        let (at, heading) = along(&points, 15.0).unwrap();
        assert!(at.distance(Vec3::new(10.0, 0.0, 5.0)) < 1e-4);
        assert!(heading.distance(Vec3::Z) < 1e-4);
        assert_eq!(along(&points, 99.0).unwrap().0, points[2]);
    }

    #[test]
    fn the_sea_search_keeps_off_the_shore_and_goes_round_an_island() {
        // A sea 200 cells across with an island in its middle.
        let side = [60, 60];
        let island = |[x, z]: [usize; 2]| (x as f32 - 30.0).powi(2) + (z as f32 - 30.0).powi(2) < 64.0;
        let depth: Vec<f32> =
            (0..side[1]).flat_map(|z| (0..side[0]).map(move |x| if island([x, z]) { -5.0 } else { 10.0 })).collect();
        let far = distance_from_land(&depth, side);
        let clear = |at: [usize; 2]| far[at[1] * side[0] + at[0]] >= 3.0;
        let path = search(side, [5, 30], [55, 30], &clear).expect("round the island");
        assert!(path.iter().all(|&at| clear(at)));
        assert!(path.iter().any(|at| at[1].abs_diff(30) >= 10), "it goes round");
    }
}
