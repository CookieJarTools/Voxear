//! What each node of a VoxShade shader makes, as a picture on its box: its
//! value on a small ball, worked out on the CPU at every pixel of the ball
//! (`voxear_voxshade::eval`), the way the shader would work it out on the
//! GPU. A number shows grey, two numbers red and green, three or four as
//! colour; the Surface Output shows the ball lit by its colour and what it
//! gives off. Pictures read as white, as a material that has not picked
//! one draws them, and a sub-graph's inputs as their defaults. Slang
//! written by hand runs only on the GPU, so what a Slang node gives, and
//! what is worked out from it, shows striped.
//!
//! Every node's picture is one cell of one texture, made again when the
//! graph changes what it works out (not when nodes only move), and a few
//! times a second while the graph reads the time.

use std::collections::HashMap;

use voxear_render::{Renderer, TextureSource};
use voxear_rhi::TextureId;
use voxear_ui::ImageRef;
use voxear_voxshade::{
    Graph, Input, Library, NodeId, NodeKind, PropertyKind, Stage, Ty,
    eval::{Inputs, run},
    ir::{Given, Op},
    lower::lower_all,
};

/// How many pixels a side each picture is, and the gap round it in the
/// texture, so filtering never takes a neighbour's colour.
pub const SIDE: u32 = 72;
const GUTTER: u32 = 2;
/// How often a picture that shows the time is made again, in seconds.
const ANIMATE_EVERY: f32 = 1.0 / 15.0;

/// The way to the light the previews are lit by, and to the camera.
const TO_LIGHT: [f32; 3] = [-0.5, 0.65, -0.57];
const VIEW: [f32; 3] = [0.0, 0.0, -1.0];

/// The pictures made, as RGBA pixels, a node each, `SIDE` square.
pub type Pictures = Vec<(NodeId, Vec<u8>)>;

#[derive(Default)]
pub struct Previews {
    atlas: Option<TextureId>,
    cells: HashMap<NodeId, ImageRef>,
    /// What the pictures were made from, and when.
    made_for: String,
    made_at: f32,
    animated: bool,
}

impl Previews {
    /// Makes the pictures again when `graph` works out other values than
    /// they show, or the time has moved on for a graph that reads it.
    /// Whether they changed.
    pub fn update(&mut self, renderer: &mut Renderer, graph: &Graph, library: &dyn Library, time: f32) -> bool {
        let key = what_it_works_out(graph, library);
        let due = self.animated && time - self.made_at >= ANIMATE_EVERY;
        if key == self.made_for && !due {
            return false;
        }
        let (pictures, animated) = draw(graph, Some(library), time);
        (self.made_for, self.made_at, self.animated) = (key, time, animated);
        self.upload(renderer, &pictures);
        true
    }

    pub fn picture(&self, node: NodeId) -> Option<ImageRef> {
        self.cells.get(&node).copied()
    }

    pub fn clear(&mut self, renderer: &mut Renderer) {
        if let Some(atlas) = self.atlas.take() {
            renderer.free_texture(atlas);
        }
        self.cells.clear();
        self.made_for.clear();
    }

    fn upload(&mut self, renderer: &mut Renderer, pictures: &Pictures) {
        let cell = SIDE + GUTTER * 2;
        let across = (pictures.len() as f32).sqrt().ceil().max(1.0) as u32;
        let down = (pictures.len() as u32).div_ceil(across).max(1);
        let (width, height) = (across * cell, down * cell);
        let mut rgba = vec![0u8; (width * height * 4) as usize];
        let mut cells = HashMap::new();
        for (place, (node, pixels)) in pictures.iter().enumerate() {
            let (x0, y0) = ((place as u32 % across) * cell + GUTTER, (place as u32 / across) * cell + GUTTER);
            for y in 0..SIDE {
                let from = (y * SIDE * 4) as usize;
                let to = (((y0 + y) * width + x0) * 4) as usize;
                rgba[to..to + (SIDE * 4) as usize].copy_from_slice(&pixels[from..from + (SIDE * 4) as usize]);
            }
            let (w, h) = (width as f32, height as f32);
            let uv = [x0 as f32 / w, y0 as f32 / h, (x0 + SIDE) as f32 / w, (y0 + SIDE) as f32 / h];
            cells.insert(*node, uv);
        }
        let source = TextureSource { name: "VoxShade previews", width, height, rgba: &rgba, srgb: true };
        let Ok(texture) = renderer.add_texture(&source) else { return };
        if let Some(old) = self.atlas.replace(texture) {
            renderer.free_texture(old);
        }
        self.cells = cells.into_iter().map(|(node, uv)| (node, ImageRef { texture: texture.0, uv })).collect();
    }
}

/// Everything about a graph that changes what its nodes work out, which
/// where its nodes stand does not, with the sub-graphs it uses.
fn what_it_works_out(graph: &Graph, library: &dyn Library) -> String {
    let mut placed = graph.clone();
    for node in &mut placed.nodes {
        node.at = [0.0; 2];
    }
    let used: Vec<Option<&Graph>> = graph
        .nodes
        .iter()
        .filter_map(|node| match &node.kind {
            NodeKind::SubGraph { graph, .. } => Some(library.sub_graph(*graph)),
            _ => None,
        })
        .collect();
    format!("{placed:?} {used:?}")
}

/// A point on the preview ball, at pixel (x, y): where it is, the way it
/// faces and its UV; none off the ball.
fn on_ball(x: u32, y: u32) -> Option<([f32; 3], [f32; 3], [f32; 2])> {
    let at = |p: u32| (p as f32 + 0.5) / SIDE as f32 * 2.0 - 1.0;
    let (sx, sy) = (at(x), -at(y));
    let r2 = sx * sx + sy * sy;
    if r2 > 1.0 {
        return None;
    }
    let normal = [sx, sy, -(1.0 - r2).sqrt()];
    let position = normal.map(|n| n * 0.5);
    let u = normal[2].atan2(normal[0]) / std::f32::consts::TAU + 0.5;
    let v = 0.5 - normal[1].asin() / std::f32::consts::PI;
    Some((position, normal, [u, v]))
}

/// A property's value as a material using the shader starts with it, as the GPU has it: a colour in linear light.
fn property_value(graph: &Graph, id: voxear_voxshade::PropertyId) -> [f32; 4] {
    let Some(property) = graph.property(id) else { return [0.0; 4] };
    let mut value = property.default;
    if matches!(property.kind, PropertyKind::Color | PropertyKind::ColorAlpha) {
        for channel in &mut value[..3] {
            *channel = linear(*channel);
        }
    }
    value
}

fn linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn encode(c: f32) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let s = if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0 + 0.5) as u8
}

/// The colour a value of `ty` shows as.
fn shown(ty: Ty, value: [f32; 4]) -> [f32; 3] {
    match ty {
        Ty::Float | Ty::Bool => [value[0]; 3],
        Ty::Float2 => [value[0], value[1], 0.0],
        Ty::Float3 | Ty::Float4 => [value[0], value[1], value[2]],
    }
}

/// Every node's picture, at `time`; and whether any reads the time, so would change with it.
pub fn draw(graph: &Graph, library: Option<&dyn Library>, time: f32) -> (Pictures, bool) {
    // Each node in the first stage it can run in.
    // A particle graph's nodes in its two stages; a shader's or a sub-graph's in a shader's, and then those.
    let tried: &[Stage] = if graph.particles {
        &Stage::PARTICLES
    } else {
        &[Stage::Surface, Stage::Light, Stage::Vertex, Stage::Update, Stage::Look]
    };
    let stages: Vec<(Stage, voxear_voxshade::ir::Program, HashMap<(NodeId, usize), voxear_voxshade::ir::ValueId>)> =
        tried
            .iter()
            .map(|&stage| {
                let (program, found) = lower_all(graph, stage, library);
                (stage, program, found)
            })
            .collect();
    let mut shows: Vec<(NodeId, usize, Vec<usize>)> = Vec::new();
    for node in &graph.nodes {
        let is_output = node.kind.output_stage().is_some() || node.kind == NodeKind::SubGraphOutput;
        let has_outputs = !graph.outputs_of(&node.kind).is_empty() || is_output;
        if !has_outputs {
            continue;
        }
        let in_stage = stages.iter().position(|(stage, _, found)| {
            node.kind.output_stage().is_none_or(|own| own == *stage) && found.contains_key(&(node.id, 0))
        });
        if let Some(at) = in_stage {
            let slots = match node.kind {
                NodeKind::SurfaceOutput => vec![0, 1, 5],
                NodeKind::LookOutput => vec![0, 1],
                _ => vec![0],
            };
            shows.push((node.id, at, slots));
        }
    }
    let animated = stages
        .iter()
        .any(|(_, program, _)| program.steps.iter().any(|step| step.op == Op::Given(Given::Input(Input::Time))));
    let mut pictures: Pictures =
        shows.iter().map(|(node, ..)| (*node, vec![0u8; (SIDE * SIDE * 4) as usize])).collect();
    let used: Vec<bool> = (0..stages.len()).map(|at| shows.iter().any(|(_, s, _)| *s == at)).collect();
    let mut values: Vec<Vec<[f32; 4]>> = vec![Vec::new(); stages.len()];
    for y in 0..SIDE {
        for x in 0..SIDE {
            let Some((position, normal, uv)) = on_ball(x, y) else { continue };
            let to_light = {
                let l = (TO_LIGHT[0] * TO_LIGHT[0] + TO_LIGHT[1] * TO_LIGHT[1] + TO_LIGHT[2] * TO_LIGHT[2]).sqrt();
                TO_LIGHT.map(|c| c / l)
            };
            let three = |v: [f32; 3]| [v[0], v[1], v[2], 0.0];
            let given = |what: Given| match what {
                Given::Input(Input::Uv) => [uv[0], uv[1], 0.0, 0.0],
                Given::Input(Input::Position | Input::LevelPosition) => three(position),
                Given::Input(Input::Normal) => three(normal),
                Given::Input(Input::View | Input::LightView) => three(VIEW),
                Given::Input(Input::ObjectPosition) => [0.0; 4],
                Given::Input(Input::Time) => [time; 4],
                Given::Input(Input::Thickness) => [1000.0; 4],
                Given::Input(Input::LightDirection) => three(to_light),
                Given::Input(Input::LightColor) => [1.0; 4],
                Given::Surface(0) => [0.8, 0.8, 0.8, 0.0],
                Given::Surface(1) => [1.0; 4],
                Given::Surface(2) => three(normal),
                Given::Surface(4) => [0.5; 4],
                Given::Surface(_) => [0.0; 4],
                Given::Argument(n) => graph.inputs.get(n).map_or([0.0; 4], |pin| pin.default),
                // A particle's life lived from left to right across the ball, its number from top to bottom.
                Given::Input(Input::Age) => [(position[0] + 0.5) * 2.0; 4],
                Given::Input(Input::Life) => [position[0] + 0.5; 4],
                Given::Input(Input::Seed) => [0.5 - position[1]; 4],
                Given::Input(Input::Velocity) => three(normal.map(|n| n * 2.0)),
                Given::Input(Input::DeltaTime) => [1.0 / 60.0; 4],
                Given::Input(Input::ParticleColor) => [0.8, 0.8, 0.8, 1.0],
                Given::Input(Input::ParticleSize) => [1.0; 4],
            };
            let property = |id| property_value(graph, id);
            let inputs = Inputs { given: &given, property: &property, sample: &|_, _| [1.0; 4] };
            for (at, (_, program, _)) in stages.iter().enumerate() {
                if used[at] {
                    run(program, &inputs, &mut values[at]);
                }
            }
            let pixel = ((y * SIDE + x) * 4) as usize;
            for ((node, at, slots), (_, rgba)) in shows.iter().zip(pictures.iter_mut()) {
                let (_, program, found) = &stages[*at];
                let read = |slot: usize| {
                    found.get(&(*node, slot)).map(|id| (program.step(*id).ty, values[*at][id.0 as usize]))
                };
                // Not known on the CPU: stripes.
                let unknown = slots.iter().any(|&slot| read(slot).is_some_and(|(_, v)| v.iter().any(|c| c.is_nan())));
                // A Look Output's input left as it is keeps the particle's own.
                let set = |slot: usize| {
                    graph.link_into(*node, slot).is_some()
                        || graph.node(*node).is_some_and(|n| n.literals.iter().any(|l| l.input == slot))
                };
                let color = if unknown {
                    [if (x + y) / 6 % 2 == 0 { 0.30 } else { 0.12 }; 3]
                } else if *slots == [0, 1] {
                    let base = if set(0) { read(0).map_or([0.8; 3], |(ty, v)| shown(ty, v)) } else { [0.8; 3] };
                    let alpha = if set(1) { read(1).map_or(1.0, |(_, v)| v[0].clamp(0.0, 1.0)) } else { 1.0 };
                    base.map(|c| c * alpha)
                } else if slots.len() > 1 {
                    // The surface lit, as it would be drawn: its colour in the light, and what it gives off.
                    let base = read(0).map_or([0.8; 3], |(ty, v)| shown(ty, v));
                    let glow = read(5).map_or([0.0; 3], |(ty, v)| shown(ty, v));
                    let facing = (normal[0] * to_light[0] + normal[1] * to_light[1] + normal[2] * to_light[2]).max(0.0);
                    let light = if graph.unlit { 1.0 } else { 0.15 + 0.85 * facing };
                    [0, 1, 2].map(|c| base[c] * light + glow[c])
                } else {
                    read(0).map_or([0.0; 3], |(ty, v)| shown(ty, v))
                };
                rgba[pixel..pixel + 4].copy_from_slice(&[encode(color[0]), encode(color[1]), encode(color[2]), 255]);
            }
        }
    }
    (pictures, animated)
}

#[cfg(test)]
mod tests {
    use voxear_voxshade::{MathOp, starters};

    use super::*;

    fn pixel(pictures: &Pictures, node: NodeId, x: u32, y: u32) -> [u8; 4] {
        let (_, rgba) = pictures.iter().find(|(n, _)| *n == node).expect("a picture for the node");
        let at = ((y * SIDE + x) * 4) as usize;
        [rgba[at], rgba[at + 1], rgba[at + 2], rgba[at + 3]]
    }

    #[test]
    fn each_node_shows_its_value_on_a_ball_and_the_surface_output_the_ball_lit() {
        let graph = starters::lit("Glow");
        let (pictures, animated) = draw(&graph, None, 0.0);
        assert!(!animated, "nothing reads the time");
        let uv = graph.nodes.iter().find(|n| n.kind == NodeKind::Input { input: Input::Uv }).unwrap().id;
        // Off the ball nothing; on it, the UV as red and green: more green lower down.
        assert_eq!(pixel(&pictures, uv, 0, 0), [0, 0, 0, 0]);
        let (high, low) = (pixel(&pictures, uv, SIDE / 2, SIDE / 4), pixel(&pictures, uv, SIDE / 2, SIDE * 3 / 4));
        assert!(low[1] > high[1] + 40 && high[3] == 255, "{high:?} {low:?}");
        // The white tint over a picture that reads as white: the surface lit, brighter on the side the light is.
        let out = graph.nodes.iter().find(|n| n.kind == NodeKind::SurfaceOutput).unwrap().id;
        let (lit, shaded) =
            (pixel(&pictures, out, SIDE / 4, SIDE / 3), pixel(&pictures, out, SIDE * 3 / 4, SIDE * 2 / 3));
        assert!(lit[0] > shaded[0] + 60, "{lit:?} {shaded:?}");
        // A sine of the time changes as the time does, and says so.
        let mut moving = graph.clone();
        let time = moving.add(NodeKind::Input { input: Input::Time }, [0.0; 2]);
        let sine = moving.add(NodeKind::Math { op: MathOp::Sine }, [0.0; 2]);
        moving.link(time, 0, sine, 0);
        let (at_zero, animated) = draw(&moving, None, 0.0);
        let (later, _) = draw(&moving, None, 1.5);
        assert!(animated);
        assert!(pixel(&later, sine, SIDE / 2, SIDE / 2)[0] > pixel(&at_zero, sine, SIDE / 2, SIDE / 2)[0] + 100);
    }

    #[test]
    fn where_nodes_stand_does_not_change_what_their_pictures_show() {
        let mut graph = starters::lit("Glow");
        let before = what_it_works_out(&graph, &HashMap::<voxear_assets::AssetGuid, Graph>::new());
        graph.nodes[0].at = [999.0, 1.0];
        assert_eq!(what_it_works_out(&graph, &HashMap::<voxear_assets::AssetGuid, Graph>::new()), before);
        graph.set_literal(graph.nodes[0].id, 0, [0.5; 4]);
        assert_ne!(what_it_works_out(&graph, &HashMap::<voxear_assets::AssetGuid, Graph>::new()), before);
    }

    #[test]
    fn a_slang_nodes_picture_is_striped_and_a_sub_graph_shows_its_inputs_defaults() {
        let mut graph = Graph::new("Test");
        let code = graph.add(crate::voxshade_panel::new_code_node(), [0.0; 2]);
        let normal = graph.add(NodeKind::Input { input: Input::Normal }, [0.0; 2]);
        graph.link(normal, 0, code, 0);
        let (pictures, _) = draw(&graph, None, 0.0);
        let (a, b) = (pixel(&pictures, code, SIDE / 2, SIDE / 2), pixel(&pictures, code, SIDE / 2 + 6, SIDE / 2));
        assert_ne!(a, b, "stripes, not a value");
        assert_eq!(a[0], a[2], "grey");
        // A sub-graph on its own: its input as its default, 2 here, so its power node shows a power of 2.
        let sub = starters::sub_graph("Edges");
        let input = sub.nodes.iter().find(|n| matches!(n.kind, NodeKind::SubGraphInput { .. })).unwrap().id;
        let (pictures, _) = draw(&sub, None, 0.0);
        let shown = pixel(&pictures, input, SIDE / 2, SIDE / 2);
        assert_eq!(shown[0], 255, "2, shown as far past white: {shown:?}");
        let out = sub.nodes.iter().find(|n| n.kind == NodeKind::SubGraphOutput).unwrap().id;
        let (middle, edge) = (pixel(&pictures, out, SIDE / 2, SIDE / 2), pixel(&pictures, out, 3, SIDE / 2));
        assert!(edge[0] > middle[0] + 60, "brighter edge on: {middle:?} {edge:?}");
    }

    #[test]
    fn a_particle_graphs_nodes_show_a_particles_life_across_the_ball() {
        let graph = starters::particles("Swirl");
        let (pictures, _) = draw(&graph, None, 0.0);
        let life = graph.nodes.iter().find(|n| n.kind == NodeKind::Input { input: Input::Life }).unwrap().id;
        let (young, old) = (pixel(&pictures, life, SIDE / 4, SIDE / 2), pixel(&pictures, life, SIDE * 3 / 4, SIDE / 2));
        assert!(old[0] > young[0] + 60, "life lived from left to right: {young:?} {old:?}");
        // The Look Output: the particle's own colour, fading as its life is lived.
        let look = graph.nodes.iter().find(|n| n.kind == NodeKind::LookOutput).unwrap().id;
        let (early, late) =
            (pixel(&pictures, look, SIDE / 4, SIDE / 2), pixel(&pictures, look, SIDE * 3 / 4, SIDE / 2));
        assert!(early[0] > late[0] + 60 && early[0] == early[2], "grey, faded: {early:?} {late:?}");
        let update = graph.nodes.iter().find(|n| n.kind == NodeKind::UpdateOutput).unwrap().id;
        assert!(pictures.iter().any(|(node, _)| *node == update), "the force shown");
    }
}
