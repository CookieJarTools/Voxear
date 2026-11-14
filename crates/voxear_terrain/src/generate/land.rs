//! The lie of a biome's land: each biome can shape its own country, not
//! only lay its blocks over it. Plains flatten the ground and roll it a
//! little; Hills raise rounded hills with valleys cut down them; Mountains
//! raise ranges of sharp peaks with deep valleys between and leaning,
//! overhanging cliffs; Tablelands raise the ground in level tables with
//! sheer walls between them; Pillars raise tall towers of rock over low
//! rolling ground; a Rim raises wide upland plains that end in a sheer wall
//! over the valley below, a lip along the top hiding the drop until one
//! stands at it. Any of them can have natural bridges spanning between
//! high places, and overhangs pushed out of their steep faces.
//!
//! ```toml
//! [land]
//! form = "mountains"
//! height = 420.0      # metres: how high its hills, peaks or tables rise
//! size = 1400.0       # metres: how far apart they stand
//! valleys = 0.6       # 0 to 1: how deep valleys are cut down its slopes
//! overhangs = 8.0     # metres: how far its cliffs lean and hang out
//! pillars = 0.0       # 0 to 1: how many of the places a pillar could stand have one
//! pillar_height = 120.0
//! bridges = 0.3       # 0 to 1: how many of the places a bridge could span have one
//! ```
//!
//! A world of blocks shapes its generator's ground by the biomes it lists
//! ([`add_land`], [`add_land_volume`]). Each biome's land is made from the
//! generator's own ground, so coasts and islands stay where they were: the
//! land rises out of the ground away from the sea, over the first tens of
//! metres above it. Where it is, is read from the climate as the biome's
//! own is, by a Climate Mask of its temperature and humidity eased in over
//! a few degrees, so one land fades into the next across the border
//! between them. Rivers run down the land so shaped.
//!
//! It is done with the generator's own nodes, added to a copy of its graph:
//! Peaks, Plateaus, Noise, Spires, Arches and Overhangs, each
//! kept to its biome by a Climate Mask, so the same shapes can be built by
//! hand in a generator, and changed there.
//!
//! A world whose land lies in levels (see `nodes::levels`) lays a Levels
//! node over the generator's ground first: level areas at set heights,
//! walls between. Each biome's land then rolls over its area's floor as
//! much as the area's character says, in place of rising out of the
//! generator's own hills, and its mountains stand as lone massifs over the
//! level, not ranges over all of it. Spires stand in areas of spires and
//! natural bridges span the ravines of areas of canyons.

use serde::{Deserialize, Serialize};
use voxear_core::curve::Curve;

use super::{
    BiomeDef, Graph, NodeId, NodeKind,
    nodes::{
        Arches, Blend, ClimateMask, Constant, Falloff, FalloffShape, Levels, Math, Noise, Operation, Output, Overhangs, Peaks,
        Plateaus, RegionOf, Remap, Rivers, SlopeMask, Spires, Terrace,
    },
};
use crate::noise::NoiseStyle;

/// The kind of country a biome's land is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Landform {
    /// The generator's own ground, as it is.
    #[default]
    Keep,
    Plains,
    Hills,
    Mountains,
    Tablelands,
    Pillars,
    /// Wide upland plains that end in a sheer wall over the valley below.
    Rim,
}

impl Landform {
    pub const ALL: [Landform; 7] = [
        Landform::Keep,
        Landform::Plains,
        Landform::Hills,
        Landform::Mountains,
        Landform::Tablelands,
        Landform::Pillars,
        Landform::Rim,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Landform::Keep => "Generator's Own",
            Landform::Plains => "Plains",
            Landform::Hills => "Hills",
            Landform::Mountains => "Mountains",
            Landform::Tablelands => "Tablelands",
            Landform::Pillars => "Pillars",
            Landform::Rim => "Rim",
        }
    }
}

/// How a biome shapes its land: the kind of country, how high and how
/// wide, and what stands on it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BiomeLand {
    pub form: Landform,
    /// How high its hills, peaks, tables or rolls rise over the generator's
    /// ground, in metres.
    pub height: f32,
    /// How far apart they stand, in metres.
    pub size: f32,
    /// How deep valleys are cut down its slopes, from 0 for none to 1.
    pub valleys: f32,
    /// How far its steep faces lean and hang out, in metres.
    pub overhangs: f32,
    /// How many of the places a pillar of rock could stand have one, 0 to 1.
    pub pillars: f32,
    /// How tall the tallest pillar stands, in metres.
    pub pillar_height: f32,
    /// How many of the places a natural bridge could span have one, 0 to 1.
    pub bridges: f32,
}

impl Default for BiomeLand {
    fn default() -> Self {
        Self::of(Landform::Keep)
    }
}

impl BiomeLand {
    /// `form` as it comes: what each kind of country has unless changed.
    pub fn of(form: Landform) -> Self {
        let land = |height, size, valleys, overhangs, pillars, pillar_height, bridges| Self {
            form,
            height,
            size,
            valleys,
            overhangs,
            pillars,
            pillar_height,
            bridges,
        };
        match form {
            Landform::Keep => land(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
            Landform::Plains => land(10.0, 700.0, 0.2, 0.0, 0.0, 0.0, 0.0),
            Landform::Hills => land(70.0, 500.0, 0.45, 0.0, 0.0, 0.0, 0.0),
            Landform::Mountains => land(420.0, 1400.0, 0.55, 10.0, 0.0, 0.0, 0.35),
            Landform::Tablelands => land(110.0, 320.0, 0.0, 4.0, 0.25, 70.0, 0.5),
            Landform::Rim => land(260.0, 1800.0, 0.0, 4.0, 0.06, 130.0, 0.15),
            Landform::Pillars => land(35.0, 400.0, 0.3, 6.0, 0.7, 140.0, 0.5),
        }
    }

    pub fn is_keep(&self) -> bool {
        self.form == Landform::Keep && self.pillars <= 0.0 && self.bridges <= 0.0 && self.overhangs <= 0.0
    }
}

/// How high over the sea the generator's ground is before a biome's land
/// rises out of it in full, in metres: so the coast stays where it was.
const COAST: f32 = 2.0;
const RISE: f32 = 40.0;

/// How far a biome's land eases in past its climate's ranges: wider than
/// the blocks' borders, so hills and mountains fall away over a stretch.
const TEMPERATURE_FEATHER: f32 = 4.0;
const HUMIDITY_FEATHER: f32 = 0.12;

/// Where each biome's land lies in a graph [`add_land`] shaped: its biome's
/// place in the list and its mask, the land's height, and where the land
/// may rise at all, where it keeps clear of a world's start.
#[derive(Clone, Debug, Default)]
pub struct Shaped {
    masks: Vec<(usize, NodeId)>,
    height: Option<NodeId>,
    open: Option<NodeId>,
    /// The Levels node, where the land lies in levels.
    levels: Option<NodeId>,
}

/// Ground round a place the land keeps clear of: its middle, in metres in
/// the generator's world, and how far round it the generator's own ground
/// is kept whole. The land rises over as far again past that.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clear {
    pub middle: [f32; 2],
    pub radius: f32,
}

impl Shaped {
    pub fn is_empty(&self) -> bool {
        self.masks.is_empty() && self.levels.is_none()
    }
}

/// Shapes `graph`'s ground by the land of each of `biomes` that has any:
/// between what was its ground (what is wired into its Rivers' ground,
/// where it has its own rivers, or else into its Output's Height) and
/// where that went, each biome's land blended in by where its climate is.
/// Nothing, and an empty [`Shaped`], where no biome has land or the graph
/// has no ground wired in.
///
/// Within `clear`, where there is one, the ground is the generator's own.
/// Where the world keeps its biomes to `regions`, each biome's land lies
/// where its regions do, blended over their border, in place of where its
/// climate is.
///
/// Where its land lies in `levels`, a Levels node is laid over the ground
/// first, its areas the regions' where the regions have levels, and each
/// biome's land rolls over the levels' floors.
pub fn add_land(
    graph: &mut Graph,
    biomes: &[BiomeDef],
    sea_level: f32,
    clear: Option<Clear>,
    regions: Option<&std::sync::Arc<super::Regions>>,
    levels: Option<&Levels>,
) -> Shaped {
    let mut shaped = Shaped::default();
    if levels.is_none() && biomes.iter().all(|biome| biome.land.is_keep()) {
        return shaped;
    }
    let Some(output) = graph.output().map(|node| (node.id, node.at)) else { return shaped };
    let Some(into) = ground_into(graph, output.0) else { return shaped };
    let Some(base) = graph.link_into(into.0, into.1).copied() else { return shaped };
    let mut g = Adder { graph, at: [output.1[0] - 1400.0, output.1[1] + 900.0], row: 0.0 };

    // The climate as it is over the ground before it is shaped, so a biome's land does not move itself: where the
    // biomes keep to regions, none is needed.
    let (temperature, humidity) = match regions {
        Some(_) => (None, None),
        None => g.climate(output.0, base.from, base.output),
    };
    let rising = g.add(NodeKind::Curve(Remap {
        curve: Curve::new(&[[0.0, 0.0], [0.5, 0.3], [1.0, 1.0]]),
        low: sea_level + COAST,
        high: sea_level + COAST + RISE,
    }));
    g.wire(base.from, base.output, rising, 0);
    let rising = match clear.filter(|clear| clear.radius > 0.0) {
        Some(clear) => {
            let round = g.add(NodeKind::Falloff(Falloff {
                shape: FalloffShape::Round,
                x: clear.middle[0],
                z: clear.middle[1],
                radius: clear.radius,
                feather: clear.radius,
            }));
            let open = g.add(NodeKind::Math(Math { operation: Operation::Subtract, a: 1.0, b: 0.0 }));
            g.wire(round, 0, open, 1);
            shaped.open = Some(open);
            g.math2(Operation::Multiply, (rising, 0), (open, 0))
        }
        None => rising,
    };

    // The levels: their floors, walls between, and how much each place's land rolls.
    let level = levels.map(|levels| {
        g.next();
        let areas = regions.and_then(|regions| regions.areas().cloned());
        let node = g.add(NodeKind::Levels(Levels { areas, ..levels.clone() }));
        g.wire(base.from, base.output, node, Levels::GROUND);
        shaped.levels = Some(node);
        (node, g.add(NodeKind::Constant(Constant { value: 0.0 })))
    });
    // Where the land lies in levels, each biome's land is made from level ground and rolls over its floor.
    let made_from = level.map_or((base.from, base.output), |(_, zero)| (zero, 0));
    let mut height = made_from;
    // Where a place is within the ranges of two biomes, the narrower is the one it is (see `biomes`): so the widest
    // is blended in first, and each narrower over it.
    let mut order: Vec<usize> = (0..biomes.len()).collect();
    order.sort_by(|&a, &b| reach(&biomes[b]).total_cmp(&reach(&biomes[a])));
    for at in order {
        let biome = &biomes[at];
        if biome.land.is_keep() {
            continue;
        }
        g.next();
        let region = regions.map(|regions| RegionOf { regions: regions.clone(), biome: at });
        let mask = g.add(NodeKind::ClimateMask(ClimateMask {
            temperature: biome.temperature,
            humidity: biome.humidity,
            temperature_feather: TEMPERATURE_FEATHER,
            humidity_feather: HUMIDITY_FEATHER,
            region,
        }));
        if let Some((from, output)) = temperature {
            g.wire(from, output, mask, ClimateMask::TEMPERATURE);
        }
        if let Some((from, output)) = humidity {
            g.wire(from, output, mask, ClimateMask::HUMIDITY);
        }
        // Kept clear of a world's start, where the land is not made at all.
        let mask = match shaped.open {
            Some(open) => g.math2(Operation::Multiply, (mask, 0), (open, 0)),
            None => mask,
        };
        shaped.masks.push((at, mask));
        if biome.land.form == Landform::Keep {
            continue;
        }
        let land = match level {
            Some(_) => g.land(&biome.land, made_from, rising, 0.0, true),
            None => g.land(&biome.land, made_from, rising, sea_level, false),
        };
        let blend = g.add(NodeKind::Blend(Blend::default()));
        g.wire(height.0, height.1, blend, Blend::A);
        g.wire(land, 0, blend, Blend::B);
        g.wire(mask, 0, blend, Blend::MASK);
        height = (blend, 0);
    }
    if let Some((levels, _)) = level {
        // The floors, and each biome's land over them as much as its area rolls; the generator's own ground within
        // the world's start.
        g.next();
        let rolls = g.math2(Operation::Multiply, height, (levels, Levels::RELIEF));
        let ground = g.math2(Operation::Add, (levels, Levels::HEIGHT), (rolls, 0));
        height = match shaped.open {
            Some(open) => {
                let blend = g.add(NodeKind::Blend(Blend::default()));
                g.wire(base.from, base.output, blend, Blend::A);
                g.wire(ground, 0, blend, Blend::B);
                g.wire(open, 0, blend, Blend::MASK);
                (blend, 0)
            }
            None => (ground, 0),
        };
        // The climate cools with the height over each area's floor, not with the floor's own, so a high level is as
        // green as a low one and only what rises over it is cold.
        let climate = g.graph.link_into(output.0, Output::TEMPERATURE).map(|link| link.from);
        if let Some(climate) = climate.filter(|&node| g.graph.node(node).is_some_and(|node| matches!(node.kind, NodeKind::Climate(_)))) {
            let over = g.math2(Operation::Subtract, height, (levels, Levels::FLOOR));
            let low = levels_low(&g.graph, levels);
            let over = g.math(Operation::Add, (over, 0), low);
            g.graph.disconnect(climate, super::Climate::HEIGHT_IN);
            g.wire(over, 0, climate, super::Climate::HEIGHT_IN);
        }
        // The walls and their rubble are bare rock, with whatever else the generator says is.
        let rock = match g.graph.link_into(output.0, Output::ROCK).copied() {
            Some(link) => {
                let both = g.math2(Operation::Max, (link.from, link.output), (levels, Levels::WALLS));
                g.graph.disconnect(output.0, Output::ROCK);
                (both, 0)
            }
            None => (levels, Levels::WALLS),
        };
        g.wire(rock.0, rock.1, output.0, Output::ROCK);
    }
    if height.0 != base.from {
        g.graph.disconnect(into.0, into.1);
        g.wire(height.0, height.1, into.0, into.1);
        shaped.height = Some(height.0);
    }
    shaped
}

/// The lowest level's height of the Levels node `levels` in `graph`.
fn levels_low(graph: &Graph, levels: NodeId) -> f32 {
    match graph.node(levels).map(|node| &node.kind) {
        Some(NodeKind::Levels(levels)) => levels.areas.as_ref().map_or(levels.low, |areas| areas.plan.low),
        _ => 0.0,
    }
}

/// Stands each biome's pillars and bridges on the ground of `graph`, and
/// pushes its overhangs out of its steep faces, where [`add_land`] said
/// its land lies: after every Solid of the graph, so its caves are cut
/// through them as through the rest; or, with none, over its Output's
/// Volume. False where it has no volume to stand them in.
pub fn add_land_volume(graph: &mut Graph, biomes: &[BiomeDef], shaped: &Shaped) -> bool {
    if shaped.is_empty() {
        return false;
    }
    let Some(output) = graph.output().map(|node| (node.id, node.at)) else { return false };
    let solids: Vec<NodeId> =
        graph.nodes.iter().filter(|node| matches!(node.kind, NodeKind::Solid(_))).map(|node| node.id).collect();
    let starts: Vec<(NodeId, usize)> = if solids.is_empty() {
        match graph.link_into(output.0, Output::VOLUME) {
            Some(link) => vec![(link.from, link.output)],
            None => return false,
        }
    } else {
        solids.into_iter().map(|solid| (solid, 0)).collect()
    };
    let mut g = Adder { graph, at: [output.1[0] - 1400.0, output.1[1] + 1900.0], row: 0.0 };
    // How steep the land is, read once for every biome's overhangs.
    let mut steep = None;
    for start in starts {
        // Everything the start fed is fed from the end of the chain instead.
        let fed: Vec<(NodeId, usize)> = g
            .graph
            .links
            .iter()
            .filter(|link| link.from == start.0 && link.output == start.1)
            .map(|link| (link.to, link.input))
            .collect();
        let mut ground = start;
        for &(at, mask) in &shaped.masks {
            let land = &biomes[at].land;
            g.next();
            if land.pillars > 0.0 {
                let tallest = land.pillar_height.max(12.0);
                let spires = Spires {
                    size_from: (tallest * 0.16).clamp(10.0, 50.0),
                    size_to: (tallest * 0.4).clamp(16.0, 100.0),
                    height_from: tallest * 0.45,
                    height_to: tallest,
                    spacing: (tallest * 0.9).clamp(60.0, 400.0),
                    often: land.pillars.min(1.0),
                    seed: 0x5917_E5 + at as u32,
                    ..Spires::default()
                };
                let spires = g.add(NodeKind::Spires(spires));
                g.wire(ground.0, ground.1, spires, Spires::GROUND);
                g.wire(mask, 0, spires, Spires::MASK);
                ground = (spires, 0);
            }
            if land.bridges > 0.0 {
                let span = (land.size * 0.25).clamp(50.0, 220.0);
                let arches = Arches {
                    span_from: span * 0.5,
                    span_to: span,
                    thickness: (span * 0.07).clamp(5.0, 14.0),
                    width: (span * 0.16).clamp(10.0, 30.0),
                    often: land.bridges.min(1.0),
                    windows: land.bridges.min(1.0) * 0.6,
                    seed: 0xA2C4_E5 + at as u32,
                };
                let arches = g.add(NodeKind::Arches(arches));
                g.wire(ground.0, ground.1, arches, Arches::GROUND);
                g.wire(mask, 0, arches, Arches::MASK);
                ground = (arches, 0);
            }
            if land.overhangs > 0.0 {
                // Pushed out of steep faces alone, so level ground stays walkable.
                let steep = *steep.get_or_insert_with(|| {
                    let steep = g.add(NodeKind::SlopeMask(SlopeMask::default()));
                    if let Some(height) = shaped.height {
                        g.wire(height, 0, steep, 0);
                    }
                    steep
                });
                // The steep ground within its biome: a blend from nothing, so where the biome is not, how steep the
                // ground is is never read.
                let kept = g.add(NodeKind::Blend(Blend { a: 0.0, b: 1.0, mask: 0.0 }));
                g.wire(steep, 0, kept, Blend::B);
                g.wire(mask, 0, kept, Blend::MASK);
                let amount = land.overhangs.min(60.0);
                let overhangs = Overhangs {
                    amount,
                    size: (amount * 1.4).max(10.0),
                    seed: 0x0FE2_A5 + at as u32,
                    ..Overhangs::default()
                };
                let overhangs = g.add(NodeKind::Overhangs(overhangs));
                g.wire(ground.0, ground.1, overhangs, Overhangs::GROUND);
                g.wire(kept, 0, overhangs, Overhangs::MASK);
                ground = (overhangs, 0);
            }
        }
        if let Some(levels) = shaped.levels {
            // Fields of spires in the areas of spires, and natural bridges over the ravines of the areas of canyons.
            g.next();
            let spires = Spires {
                size_from: 16.0,
                size_to: 55.0,
                height_from: 60.0,
                height_to: 210.0,
                spacing: 150.0,
                often: 0.75,
                seed: 0x1E7E_5917,
                ..Spires::default()
            };
            let spires = g.add(NodeKind::Spires(spires));
            g.wire(ground.0, ground.1, spires, Spires::GROUND);
            g.wire(levels, Levels::SPIRES, spires, Spires::MASK);
            let arches = Arches {
                span_from: 40.0,
                span_to: 180.0,
                thickness: 11.0,
                width: 22.0,
                often: 1.0,
                windows: 0.3,
                seed: 0x1E7E_A2C4,
            };
            let arches = g.add(NodeKind::Arches(arches));
            g.wire(spires, 0, arches, Arches::GROUND);
            g.wire(levels, Levels::BRIDGES, arches, Arches::MASK);
            ground = (arches, 0);
        }
        for (to, input) in fed {
            g.graph.disconnect(to, input);
            g.wire(ground.0, ground.1, to, input);
        }
    }
    true
}

/// How much of the climate a biome's ranges take in, as a world's biomes
/// weigh it (see `Biomes::new` and `BiomeDef::reach`): its temperatures
/// in shares of 15 °C and its humidities in shares of 0.3.
fn reach(biome: &BiomeDef) -> f32 {
    let span = |range: [f32; 2], spread: f32| ((range[1] - range[0]).abs() / spread).min(8.0);
    span(biome.temperature, 15.0) * span(biome.humidity, 0.3)
}

/// Where a graph's ground goes: into its own Rivers' ground, where its
/// Output's Height comes from one, or else into its Output's Height.
fn ground_into(graph: &Graph, output: NodeId) -> Option<(NodeId, usize)> {
    let height = graph.link_into(output, Output::HEIGHT)?;
    let rivers = graph.node(height.from).filter(|node| matches!(node.kind, NodeKind::Rivers(_)));
    Some(match rivers {
        Some(rivers) if graph.link_into(rivers.id, Rivers::GROUND).is_some() => (rivers.id, Rivers::GROUND),
        _ => (output, Output::HEIGHT),
    })
}

/// Adds nodes to a graph in columns, as a person would lay them out.
struct Adder<'a> {
    graph: &'a mut Graph,
    at: [f32; 2],
    row: f32,
}

impl Adder<'_> {
    fn add(&mut self, kind: NodeKind) -> NodeId {
        let at = [self.at[0], self.at[1] + self.row * 190.0];
        self.row += 1.0;
        self.graph.add(kind, at)
    }

    fn next(&mut self) {
        self.at[0] += 280.0;
        self.row = 0.0;
    }

    fn wire(&mut self, from: NodeId, output: usize, to: NodeId, input: usize) {
        let wired = self.graph.connect(from, output, to, input);
        debug_assert!(wired, "a biome's land wires from {from:?} into {to:?}");
    }

    fn math(&mut self, operation: Operation, a: (NodeId, usize), b: f32) -> NodeId {
        let node = self.add(NodeKind::Math(Math { operation, a: 0.0, b }));
        self.wire(a.0, a.1, node, 0);
        node
    }

    fn math2(&mut self, operation: Operation, a: (NodeId, usize), b: (NodeId, usize)) -> NodeId {
        let node = self.add(NodeKind::Math(Math { operation, a: 0.0, b: 0.0 }));
        self.wire(a.0, a.1, node, 0);
        self.wire(b.0, b.1, node, 1);
        node
    }

    /// The climate over the ground `base` makes: a copy of the Climate
    /// node wired into the Output, cooled by that ground, or whatever is
    /// wired into the Output as it is, where that is no Climate node. Its
    /// temperature and humidity, each where there is one.
    #[allow(clippy::type_complexity)]
    fn climate(
        &mut self,
        output: NodeId,
        base: NodeId,
        base_output: usize,
    ) -> (Option<(NodeId, usize)>, Option<(NodeId, usize)>) {
        use super::Climate as ClimateNode;
        let temperature = self.graph.link_into(output, Output::TEMPERATURE).map(|link| (link.from, link.output));
        let humidity = self.graph.link_into(output, Output::HUMIDITY).map(|link| (link.from, link.output));
        let graph = &*self.graph;
        let climate = [temperature, humidity]
            .into_iter()
            .flatten()
            .filter_map(|(node, _)| graph.node(node))
            .find(|node| matches!(node.kind, NodeKind::Climate(_)));
        let Some((id, kind)) = climate.map(|node| (node.id, node.kind.clone())) else {
            return (temperature, humidity);
        };
        let own = self.add(kind);
        self.wire(base, base_output, own, ClimateNode::HEIGHT_IN);
        let moved = |from: Option<(NodeId, usize)>, out: usize| match from {
            Some((node, _)) if node == id => Some((own, out)),
            other => other,
        };
        (moved(temperature, ClimateNode::TEMPERATURE_OUT), moved(humidity, ClimateNode::HUMIDITY_OUT))
    }

    /// The height of `land` made from the ground `base`, risen out of it
    /// by `rising` (0 at the coast, 1 inland). Where the land lies in
    /// levels, `levelled`, its mountains stand as lone massifs, not ranges.
    fn land(&mut self, land: &BiomeLand, base: (NodeId, usize), rising: NodeId, sea_level: f32, levelled: bool) -> NodeId {
        let height = land.height.clamp(0.0, 4000.0);
        let size = land.size.clamp(20.0, 20000.0);
        let raised = match land.form {
            Landform::Keep => return base.0,
            Landform::Plains => {
                // The ground over the sea brought down to a third of its height, rolled a little.
                let over = self.math(Operation::Subtract, base, sea_level);
                let up = self.math(Operation::Max, (over, 0), 0.0);
                let down = self.math(Operation::Min, (over, 0), 0.0);
                let up = self.math(Operation::Multiply, (up, 0), 0.35);
                let flat = self.math2(Operation::Add, (up, 0), (down, 0));
                let flat = self.math(Operation::Add, (flat, 0), sea_level);
                let rolls = self.add(NodeKind::Noise(Noise {
                    style: NoiseStyle::Fractal,
                    size,
                    octaves: 3,
                    low: 0.0,
                    high: height,
                }));
                let rolls = self.math2(Operation::Multiply, (rolls, 0), (rising, 0));
                self.math2(Operation::Add, (flat, 0), (rolls, 0))
            }
            Landform::Hills | Landform::Pillars => {
                let octaves = if land.form == Landform::Hills { 4 } else { 3 };
                let hills = self.add(NodeKind::Noise(Noise {
                    style: NoiseStyle::Fractal,
                    size,
                    octaves,
                    low: 0.0,
                    high: height,
                }));
                let hills = self.curve(hills, height);
                let hills = self.math2(Operation::Multiply, (hills, 0), (rising, 0));
                self.math2(Operation::Add, base, (hills, 0))
            }
            Landform::Mountains => {
                let peaks = self.add(NodeKind::Peaks(Peaks {
                    size,
                    height,
                    base: 0.0,
                    ranges: 0.65,
                    sharpness: 0.75,
                    ridges: 0.6,
                    seed: 0,
                }));
                // Bands of cliff across their flanks in places, as harder layers of rock stand out: the peaks cut
                // into steps where a broad pattern says, and left as they were elsewhere.
                let step = (height / 14.0).clamp(8.0, 60.0);
                let stepped = self.add(NodeKind::Terrace(Terrace { step, sharpness: 0.55 }));
                self.wire(peaks, Peaks::HEIGHT, stepped, 0);
                let bands = self.add(NodeKind::Noise(Noise {
                    style: NoiseStyle::Fractal,
                    size: size * 0.5,
                    octaves: 2,
                    low: -0.6,
                    high: 1.6,
                }));
                let bands = self.add_clamped(bands);
                let cliffs = self.add(NodeKind::Blend(Blend::default()));
                self.wire(peaks, Peaks::HEIGHT, cliffs, Blend::A);
                self.wire(stepped, 0, cliffs, Blend::B);
                self.wire(bands, 0, cliffs, Blend::MASK);
                let peaks = self.math2(Operation::Multiply, (cliffs, 0), (rising, 0));
                let peaks = match levelled {
                    // Lone massifs, where a broad pattern is high, and level ground between.
                    true => {
                        let broad = self.add(NodeKind::Noise(Noise {
                            style: NoiseStyle::Fractal,
                            size: size * 1.6,
                            octaves: 2,
                            low: 0.0,
                            high: 1.0,
                        }));
                        let massifs = self.add(NodeKind::Curve(Remap {
                            curve: Curve::new(&[[0.0, 0.0], [0.56, 0.0], [0.7, 1.0], [1.0, 1.0]]),
                            low: 0.0,
                            high: 1.0,
                        }));
                        self.wire(broad, 0, massifs, 0);
                        // Cut down, not scaled: the peaks' own jagged tops rise out of the level ground, no dome.
                        let gone = self.add(NodeKind::Math(Math { operation: Operation::Subtract, a: 1.0, b: 0.0 }));
                        self.wire(massifs, 0, gone, 1);
                        let gone = self.math(Operation::Multiply, (gone, 0), height);
                        let cut = self.math2(Operation::Subtract, (peaks, 0), (gone, 0));
                        self.math(Operation::Max, (cut, 0), 0.0)
                    }
                    false => peaks,
                };
                self.math2(Operation::Add, base, (peaks, 0))
            }
            Landform::Rim => {
                // Two levels: an upland and the valley it overlooks, nearly half the land, and between them a wall
                // standing almost sheer over a slope of fallen rock, a lip along its top that hides the drop until
                // one stands at it, and a ramp up it here and there.
                let rim = self.add(NodeKind::Plateaus(Plateaus {
                    size,
                    levels: 2,
                    low: 0.0,
                    high: height,
                    wall: 12.0,
                    ramps: 0.08,
                    rim: 3.0,
                    talus: (height * 0.3).min(150.0),
                    valleys: 0.45,
                    river: 0.0,
                    canyon: 0.0,
                    wander: 0.7,
                    ..Plateaus::default()
                }));
                let rim = self.math2(Operation::Multiply, (rim, Plateaus::HEIGHT), (rising, 0));
                self.math2(Operation::Add, base, (rim, 0))
            }
            Landform::Tablelands => {
                let tables = self.add(NodeKind::Plateaus(Plateaus {
                    size,
                    low: 0.0,
                    high: height,
                    river: 0.0,
                    canyon: 0.0,
                    ..Plateaus::default()
                }));
                let tables = self.math2(Operation::Multiply, (tables, Plateaus::HEIGHT), (rising, 0));
                self.math2(Operation::Add, base, (tables, 0))
            }
        };
        if land.valleys <= 0.0 {
            return raised;
        }
        // Valleys along the lines of a ridged noise, cut V-shaped into the land as it rises: each place read once.
        // Cutting them as water would (a Gullies node) reads all the ground under it again and again, which costs
        // many times what the land itself does.
        let noise = self.add(NodeKind::Noise(Noise {
            style: NoiseStyle::Ridged,
            size: size * 0.7,
            octaves: 2,
            low: 0.0,
            high: 1.0,
        }));
        let lines = self.add(NodeKind::Curve(Remap {
            curve: Curve::new(&[[0.0, 0.0], [0.55, 0.0], [0.8, 0.3], [1.0, 1.0]]),
            low: 0.0,
            high: 1.0,
        }));
        self.wire(noise, 0, lines, 0);
        let depth = (height * 0.45).max(6.0) * land.valleys.min(1.0);
        let cut = self.math(Operation::Multiply, (lines, 0), depth);
        let cut = self.math2(Operation::Multiply, (cut, 0), (rising, 0));
        self.math2(Operation::Subtract, (raised, 0), (cut, 0))
    }

    /// `node` kept from 0 to 1.
    fn add_clamped(&mut self, node: NodeId) -> NodeId {
        let low = self.math(Operation::Max, (node, 0), 0.0);
        self.math(Operation::Min, (low, 0), 1.0)
    }

    /// Rolling noise from 0 to `height` made rounder: low ground broad, the
    /// tops rounded.
    fn curve(&mut self, noise: NodeId, height: f32) -> NodeId {
        let node = self.add(NodeKind::Curve(Remap {
            curve: Curve::new(&[[0.0, 0.0], [0.35, 0.08], [0.7, 0.6], [1.0, 1.0]]),
            low: height * 0.15,
            high: height * 0.85,
        }));
        self.wire(noise, 0, node, 0);
        self.math(Operation::Multiply, (node, 0), height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{OwnVoxels, WorldGen, starters};

    /// A starter's generator, and a grid of places over its land `side`
    /// a side, `step` metres apart.
    fn hills(side: usize, step: f32) -> (WorldGen, Vec<f32>, Vec<f32>) {
        let starter = starters().into_iter().find(|starter| starter.name == "Rolling Hills").expect("the starter");
        let world = starter.world(7, 1.0);
        let (xs, zs) =
            (0..side * side).map(|at| ((at % side) as f32 * step - 3000.0, (at / side) as f32 * step - 3000.0)).unzip();
        (world, xs, zs)
    }

    fn heights(world: &WorldGen, xs: &[f32], zs: &[f32]) -> Vec<f32> {
        let mut heights = vec![0.0; xs.len()];
        world.heights(xs, zs, &mut heights);
        heights
    }

    /// Two biomes splitting `world`'s land by how warm it is, `share` of its
    /// places in the colder, the warmer with `land`; the warmth of each
    /// place, and the warmth they split at.
    fn split(world: &WorldGen, xs: &[f32], zs: &[f32], share: f32, land: BiomeLand) -> ([BiomeDef; 2], Vec<f32>, f32) {
        let heights = heights(world, xs, zs);
        let (mut warmth, mut wetness) = (vec![0.0; xs.len()], vec![0.0; xs.len()]);
        world.climates(xs, zs, &heights, &mut warmth, &mut wetness);
        let mut sorted = warmth.clone();
        sorted.sort_by(f32::total_cmp);
        let middle = sorted[(sorted.len() as f32 * share) as usize];
        let cold = BiomeDef { temperature: [-60.0, middle], humidity: [0.0, 1.0], ..BiomeDef::default() };
        let warm = BiomeDef { temperature: [middle, 60.0], humidity: [0.0, 1.0], land, ..BiomeDef::default() };
        ([cold, warm], warmth, middle)
    }

    #[test]
    fn a_biomes_mountains_rise_where_it_lies_and_the_ground_elsewhere_is_left_as_it_was() {
        let (world, xs, zs) = hills(100, 60.0);
        let (biomes, warmth, middle) = split(&world, &xs, &zs, 0.5, BiomeLand::of(Landform::Mountains));
        let mut graph = world.graph().clone();
        let shaped = add_land(&mut graph, &biomes, world.sea_level(), None, None, None);
        assert!(!shaped.is_empty());
        let landed = world.with_graph(graph);
        assert!(landed.faults().is_empty(), "{:?}", landed.faults());
        let (before, after) = (heights(&world, &xs, &zs), heights(&landed, &xs, &zs));
        let inland = |at: usize| before[at] > world.sea_level() + COAST + RISE;
        // Well inside the cold biome the ground is as it was, but where a river runs a new way.
        let cold: Vec<usize> = (0..xs.len()).filter(|&at| warmth[at] < middle - TEMPERATURE_FEATHER - 0.5).collect();
        let kept = cold.iter().filter(|&&at| (after[at] - before[at]).abs() < 0.5).count();
        assert!(cold.len() > 300 && kept * 10 > cold.len() * 9, "{kept} of {} cold places kept", cold.len());
        // Well inside the warm biome's land, mountains: high on the whole, and some of them very high.
        let warm: Vec<usize> =
            (0..xs.len()).filter(|&at| warmth[at] > middle + TEMPERATURE_FEATHER + 0.5 && inland(at)).collect();
        let rises: Vec<f32> = warm.iter().map(|&at| after[at] - before[at]).collect();
        let mean = rises.iter().sum::<f32>() / rises.len().max(1) as f32;
        let most = rises.iter().copied().fold(0.0, f32::max);
        assert!(warm.len() > 100, "{} warm places inland", warm.len());
        assert!(mean > 30.0 && most > 150.0, "mountains rise {mean:.0} m on the whole, {most:.0} m at most");
        // The coast stays where it was: nothing under the sea is raised over it.
        let raised = (0..xs.len()).filter(|&at| before[at] < world.sea_level() - 1.0 && after[at] > world.sea_level());
        assert_eq!(raised.count(), 0, "sea floor raised into land");
    }

    #[test]
    fn a_rim_is_level_upland_ending_in_a_sheer_wall_over_the_valley() {
        let (world, _, _) = hills(1, 1.0);
        let land = BiomeLand::of(Landform::Rim);
        let biome = BiomeDef { temperature: [-60.0, 60.0], humidity: [0.0, 1.0], land, ..BiomeDef::default() };
        let mut graph = world.graph().clone();
        add_land(&mut graph, std::slice::from_ref(&biome), world.sea_level(), None, None, None);
        let rimmed = world.with_graph(graph);
        // Rows 4 m apart over 6 km of land: where the ground drops most over 8 m.
        let (mut sheer, mut level, mut inland) = (0, 0, 0);
        for row in 0..30 {
            let z = row as f32 * 200.0 - 3000.0;
            let xs: Vec<f32> = (0..1500).map(|i| i as f32 * 4.0 - 3000.0).collect();
            let zs = vec![z; xs.len()];
            let (before, after) = (heights(&world, &xs, &zs), heights(&rimmed, &xs, &zs));
            for i in 2..xs.len() {
                if before[i] < world.sea_level() + COAST + RISE {
                    continue;
                }
                inland += 1;
                let drop = (after[i] - after[i - 2]).abs();
                if drop > land.height * 0.2 {
                    sheer += 1;
                } else if (after[i] - after[i - 1]).abs() - (before[i] - before[i - 1]).abs() < 0.5 {
                    level += 1;
                }
            }
        }
        assert!(sheer > 10, "{sheer} places a wall drops over a fifth of the rim's height in 8 m");
        assert!(level * 10 > inland * 7, "{level} of {inland} inland places as level as the ground was");
    }

    #[test]
    fn where_biomes_keep_to_regions_their_land_does_too_one_land_until_the_next() {
        use crate::generate::{Biomes, Regions};
        let (world, xs, zs) = hills(120, 50.0);
        let mountains = BiomeLand::of(Landform::Mountains);
        // Half the world's warmth each, as regions 600 m apart.
        let (biomes, ..) = split(&world, &xs, &zs, 0.5, mountains);
        let world = std::sync::Arc::new(world);
        let regions = std::sync::Arc::new(Regions::new(600.0, 96.0, Biomes::new(biomes.to_vec(), 15.0, 0.3, 7), world.clone()));
        let mut graph = world.graph().clone();
        add_land(&mut graph, &biomes, world.sea_level(), None, Some(&regions), None);
        let landed = world.with_graph(graph);
        let (before, after) = (heights(&world, &xs, &zs), heights(&landed, &xs, &zs));
        let inland = |at: usize| before[at] > world.sea_level() + COAST + RISE;
        let weight = |at: usize| regions.weight(xs[at], zs[at], 1);
        // Inside the mountains' regions they rise; inside the others the ground is as it was, but where a river runs.
        let within: Vec<usize> = (0..xs.len()).filter(|&at| weight(at) > 0.999 && inland(at)).collect();
        let without: Vec<usize> = (0..xs.len()).filter(|&at| weight(at) < 0.001).collect();
        assert!(within.len() > 200 && without.len() > 200, "{} in, {} out", within.len(), without.len());
        let risen = within.iter().filter(|&&at| after[at] - before[at] > 5.0).count();
        assert!(risen * 3 > within.len(), "{risen} of {} risen in the mountains' regions", within.len());
        let kept = without.iter().filter(|&&at| (after[at] - before[at]).abs() < 0.5).count();
        assert!(kept * 10 > without.len() * 9, "{kept} of {} kept outside them", without.len());
    }

    #[test]
    fn a_biomes_pillars_and_bridges_stand_where_it_lies_and_nowhere_else() {
        // The coldest and warmest of the land a good way round, and a patch of close places round each.
        let (world, xs, zs) = hills(60, 200.0);
        let before = heights(&world, &xs, &zs);
        let (mut warmth, mut wetness) = (vec![0.0; xs.len()], vec![0.0; xs.len()]);
        world.climates(&xs, &zs, &before, &mut warmth, &mut wetness);
        let land_at = |at: &usize| before[*at] > world.sea_level() + COAST + RISE;
        let by = |a: &usize, b: &usize| warmth[*a].total_cmp(&warmth[*b]);
        let coldest = (0..xs.len()).filter(land_at).min_by(by).expect("land");
        let warmest = (0..xs.len()).filter(land_at).max_by(by).expect("land");
        let middle = (warmth[coldest] + warmth[warmest]) * 0.5;
        assert!(warmth[warmest] - warmth[coldest] > 2.0 * TEMPERATURE_FEATHER + 4.0, "{middle}");
        let cold = BiomeDef { temperature: [-60.0, middle], humidity: [0.0, 1.0], ..BiomeDef::default() };
        let land = BiomeLand { pillars: 1.0, bridges: 1.0, ..BiomeLand::of(Landform::Pillars) };
        let warm = BiomeDef { temperature: [middle, 60.0], humidity: [0.0, 1.0], land, ..BiomeDef::default() };
        let biomes = [cold, warm];
        let mut graph = world.graph().clone();
        let shaped = add_land(&mut graph, &biomes, world.sea_level(), None, None, None);
        assert!(graph.add_voxels(OwnVoxels { overhangs: 0.0, caves: 0.0, roof: 0.0 }, false));
        assert!(add_land_volume(&mut graph, &biomes, &shaped));
        let landed = world.with_graph(graph);
        assert!(landed.faults().is_empty(), "{:?}", landed.faults());
        let volume = landed.volume().expect("a Volume");
        let stood = |round: usize, keep: &dyn Fn(f32) -> bool| {
            let (xs, zs): (Vec<f32>, Vec<f32>) = (0..80 * 80)
                .map(|at| (xs[round] + (at % 80) as f32 * 20.0 - 800.0, zs[round] + (at / 80) as f32 * 20.0 - 800.0))
                .unzip();
            let (now, was) = (heights(&landed, &xs, &zs), heights(&world, &xs, &zs));
            let (mut warmth, mut wetness) = (vec![0.0; xs.len()], vec![0.0; xs.len()]);
            world.climates(&xs, &zs, &was, &mut warmth, &mut wetness);
            let standing = volume.standing(&xs, &zs, &now);
            let places: Vec<usize> = (0..xs.len()).filter(|&at| keep(warmth[at])).collect();
            let over = places.iter().filter(|&&at| standing[at].iter().any(|[foot, top]| top - foot > 30.0)).count();
            (over, places.len())
        };
        // A pillar or a bridge leans or spans a little way past its own place, so the border is given room.
        let (cold, cold_places) = stood(coldest, &|warmth| warmth < middle - TEMPERATURE_FEATHER - 2.0);
        let (warm, warm_places) = stood(warmest, &|warmth| warmth > middle + TEMPERATURE_FEATHER);
        assert!(cold_places > 500 && warm_places > 500, "{cold_places} cold places, {warm_places} warm");
        assert_eq!(cold, 0, "pillars where the biome without them lies");
        assert!(warm * 100 > warm_places, "pillars over {warm} of {warm_places} warm places");
    }

    #[test]
    fn where_the_land_lies_in_levels_a_high_level_is_as_warm_as_a_low_one() {
        let (world, xs, zs) = hills(80, 150.0);
        let levels = Levels {
            size: 900.0,
            levels: 3,
            low: 10.0,
            high: 610.0,
            rolling: 0.0,
            steps: 0.0,
            canyons: 0.0,
            spires: 0.0,
            peaks: 0.0,
            basins: 0.0,
            ..Levels::default()
        };
        let biomes = [BiomeDef { temperature: [-60.0, 60.0], humidity: [0.0, 1.0], ..BiomeDef::default() }];
        let mut graph = world.graph().clone();
        add_land(&mut graph, &biomes, world.sea_level(), None, None, Some(&levels));
        let landed = world.with_graph(graph);
        assert!(landed.faults().is_empty(), "{:?}", landed.faults());
        let now = heights(&landed, &xs, &zs);
        let (mut warmth, mut wetness) = (vec![0.0; xs.len()], vec![0.0; xs.len()]);
        landed.climates(&xs, &zs, &now, &mut warmth, &mut wetness);
        let mean = |floor: f32| {
            let on: Vec<f32> = (0..xs.len()).filter(|&at| (now[at] - floor).abs() < 2.0).map(|at| warmth[at]).collect();
            (on.iter().sum::<f32>() / on.len().max(1) as f32, on.len())
        };
        let ((low, lows), (high, highs)) = (mean(10.0), mean(610.0));
        assert!(lows > 100 && highs > 100, "{lows} places low, {highs} high");
        // Without counting from the floor, 600 m higher would be several degrees colder.
        let (cold, _) = world.climate(xs[0], zs[0], 610.0);
        let (warm, _) = world.climate(xs[0], zs[0], 10.0);
        assert!(warm - cold > 3.0, "the generator cools with height: {warm} against {cold}");
        assert!((high - low).abs() < 2.5, "high {high} °C against low {low} °C");
    }
}
