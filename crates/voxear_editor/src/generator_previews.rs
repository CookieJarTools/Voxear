//! The Generator panel's previews: a small picture of what every node of
//! the open graph makes, and the ground the whole graph makes, in 3D.
//!
//! Both show one square of the generator's world, its preview area. They
//! are worked out on a thread of their own, so the editor does not wait
//! for them: each time the graph makes other numbers it is asked again,
//! and only the newest asking is finished. The pictures come back first,
//! all in one texture (the old one is freed once the new is shown). Then
//! the ground comes as a coarse grid at once, and finer, up to 257 by 257,
//! while nothing more is asked.
//!
//! A picture shows a node's first output from the least it is in the area
//! (black) to the most (white), and the node says the two numbers under
//! it. The ground is coloured by its height above the sea: water, sand,
//! green, rock and snow.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, Sender, channel},
    },
};

use voxear_core::math::{Mat4, Quat, Vec2, Vec3};
use voxear_render::{
    MaterialDesc, MaterialHandle, MeshData, MeshHandle, PreviewId, PreviewOptions, PreviewPart, Renderer, TextureId,
    TextureSource, Vertex,
};
use voxear_terrain::generate::{Link, NodeId as GraphNode, NodeKind, PreviewArea, RoomRules, WorldGen};
use voxear_ui::ImageRef;

/// How many places across a node's picture is worked out at.
const CELL: usize = 64;
/// Pixels round each picture in the texture that repeat its edge, so its neighbours do not show in
/// it when it is drawn small.
const GUTTER: usize = 2;
/// How many places across the ground is, coarsest first.
const GROUND_STEPS: [usize; 4] = [33, 65, 129, 257];
/// How far a drag turns the ground, in radians a pixel; how far it starts tipped towards the eye, to
/// be seen from well above; and how far a drag tips it either way from there.
const TURN_PER_PIXEL: f32 = 0.01;
const TIPPED: f32 = -0.5;
const TIPS: [f32; 2] = [-0.65, 0.75];
/// How close the ground is drawn: all of it in the picture however it is turned.
const ZOOM: f32 = 1.0;
/// The heights the ground's colours run to: this far above the sea is white, and this far under it
/// the darkest water.
const HIGHEST: f32 = 3000.0;
const DEEPEST: f32 = 600.0;
/// How far below its lowest ground the block of ground shown goes, as a share of its width.
const BASE: f32 = 0.04;

/// One node's picture: where it is in the texture, and the least and most the node makes over the
/// area shown.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Thumb {
    pub image: ImageRef,
    pub low: f32,
    pub high: f32,
}

/// What the previews of a generator depend on: everything but where its nodes are and what it
/// exposes.
#[derive(Clone, Debug, PartialEq)]
struct Asked {
    seed: u32,
    scale: f32,
    sea_level: f32,
    area: PreviewArea,
    nodes: Vec<(GraphNode, NodeKind)>,
    links: Vec<Link>,
    /// The rules of the cave room shown by itself, in place of the ground.
    room: Option<RoomRules>,
}

impl Asked {
    fn of(world: &WorldGen, room: Option<RoomRules>) -> Self {
        let graph = world.graph();
        Self {
            room,
            seed: world.seed(),
            scale: world.scale(),
            sea_level: world.sea_level(),
            area: world.preview_area(),
            nodes: graph.nodes.iter().map(|node| (node.id, node.kind.clone())).collect(),
            links: graph.links.clone(),
        }
    }
}

struct Request {
    generation: u64,
    world: WorldGen,
    room: Option<RoomRules>,
}

/// What the thread hands back.
enum Made {
    /// Every node's picture in one image, with where each is, and its least and most.
    Thumbs { generation: u64, size: [u32; 2], rgba: Vec<u8>, cells: Vec<(GraphNode, usize, [f32; 4], [f32; 2])> },
    /// The ground, `across` places a side.
    Ground { generation: u64, across: usize, mesh: MeshData },
}

struct Worker {
    requests: Sender<Request>,
    made: Receiver<Made>,
    /// The newest asking, which the thread stops refining for when it is no longer.
    latest: Arc<AtomicU64>,
}

/// The ground as it is shown: its mesh, how fine it is, and which asking it answers.
struct Ground {
    mesh: MeshHandle,
    across: usize,
    generation: u64,
}

#[derive(Default)]
pub(crate) struct Previews {
    worker: Option<Worker>,
    generation: u64,
    asked: Option<Asked>,
    /// The nodes' pictures: their texture, where each node's is, and which asking they answer.
    atlas: Option<TextureId>,
    thumbs: HashMap<(GraphNode, usize), Thumb>,
    thumbs_from: u64,
    ground: Option<Ground>,
    /// The ground's colours by height, and the material that draws it with them.
    look: Option<(TextureId, MaterialHandle)>,
    preview: Option<PreviewId>,
    /// How large the ground is drawn, in pixels, and how far drags have turned it.
    size: u32,
    turn: Vec2,
    dragging: Option<(f32, f32)>,
}

impl Previews {
    /// Asks for `world`'s previews when what they show changed, and takes in what has been made
    /// since. `pixels` is how large the ground shows. With `room`, a cave room those rules make
    /// is shown by itself in place of the ground. Returns whether the nodes' pictures changed.
    pub fn update(&mut self, renderer: &mut Renderer, world: &WorldGen, pixels: u32, room: Option<RoomRules>) -> bool {
        let asked = Asked::of(world, room);
        if self.asked.as_ref() != Some(&asked) {
            self.generation += 1;
            let worker = self.worker.get_or_insert_with(Worker::start);
            worker.latest.store(self.generation, Ordering::Relaxed);
            let _ = worker.requests.send(Request { generation: self.generation, world: world.clone(), room });
            self.asked = Some(asked);
        }
        let pixels = pixels.clamp(64, voxear_render::MAX_PREVIEW_SIZE);
        if pixels != self.size {
            self.size = pixels;
            self.redraw(renderer, None);
        }
        let mut changed = false;
        let made: Vec<Made> = self.worker.iter().flat_map(|worker| worker.made.try_iter()).collect();
        for made in made {
            match made {
                Made::Thumbs { generation, size, rgba, cells } if generation >= self.thumbs_from => {
                    let source = TextureSource {
                        name: "generator previews",
                        width: size[0],
                        height: size[1],
                        rgba: &rgba,
                        srgb: true,
                    };
                    let Ok(texture) = renderer.add_texture(&source) else { continue };
                    if let Some(old) = self.atlas.replace(texture) {
                        renderer.free_texture(old);
                    }
                    self.thumbs = cells
                        .into_iter()
                        .map(|(node, output, uv, [low, high])| {
                            ((node, output), Thumb { image: ImageRef { texture: texture.0, uv }, low, high })
                        })
                        .collect();
                    self.thumbs_from = generation;
                    changed = true;
                }
                Made::Ground { generation, across, mesh } => {
                    let newer = self.ground.as_ref().is_none_or(|shown| {
                        generation > shown.generation || (generation == shown.generation && across > shown.across)
                    });
                    let Some(mesh) = newer.then(|| renderer.add_mesh(&mesh).ok()).flatten() else { continue };
                    if let Some(old) = self.ground.replace(Ground { mesh, across, generation }) {
                        renderer.free_mesh(old.mesh);
                    }
                    self.redraw(renderer, Some(mesh));
                }
                Made::Thumbs { .. } => {}
            }
        }
        changed
    }

    /// A node's output's picture, once it is made.
    pub fn thumb(&self, node: GraphNode, output: usize) -> Option<Thumb> {
        self.thumbs.get(&(node, output)).copied()
    }

    /// Whether a cave room is shown by itself in place of the ground.
    pub fn shows_room(&self) -> bool {
        self.asked.as_ref().is_some_and(|asked| asked.room.is_some())
    }

    /// How many places a side the ground shown is worked out at; 0 before any is.
    pub fn ground_across(&self) -> usize {
        self.ground.as_ref().map_or(0, |ground| ground.across)
    }

    /// Whether the ground shown is the newest asked for, as fine as it gets.
    pub fn is_settled(&self) -> bool {
        let finest = GROUND_STEPS[GROUND_STEPS.len() - 1];
        self.ground.as_ref().is_some_and(|ground| ground.generation == self.generation && ground.across == finest)
            && self.thumbs_from == self.generation
    }

    /// The ground's picture, once it is drawn.
    pub fn ground_image(&mut self, renderer: &mut Renderer) -> Option<ImageRef> {
        let image = renderer.preview(self.preview?)?;
        Some(ImageRef { texture: image.texture.0, uv: image.uv })
    }

    /// Turns the ground with a drag: `pressed` while the button is held on it, with the pointer at
    /// `pointer`.
    pub fn drag(&mut self, renderer: &mut Renderer, pressed: bool, pointer: Option<(f32, f32)>) {
        match (pressed, self.dragging, pointer) {
            (true, Some(last), Some(now)) if now != last => {
                self.turn += Vec2::new(now.0 - last.0, now.1 - last.1) * TURN_PER_PIXEL;
                self.turn.y = self.turn.y.clamp(TIPS[0], TIPS[1]);
                self.dragging = Some(now);
                self.redraw(renderer, None);
            }
            (true, Some(_), _) => {}
            (true, None, now) => self.dragging = now,
            _ => self.dragging = None,
        }
    }

    /// How far drags have turned the ground: round, then towards the eye, in radians.
    pub fn turned(&self) -> Vec2 {
        self.turn
    }

    /// Forgets the nodes' pictures, as when another graph opens, whose nodes they are not.
    pub fn forget(&mut self) {
        self.thumbs.clear();
        self.asked = None;
    }

    /// Lets go of everything shown, as when the graph closes.
    pub fn clear(&mut self, renderer: &mut Renderer) {
        if let Some(preview) = self.preview.take() {
            renderer.free_preview(preview);
        }
        if let Some(ground) = self.ground.take() {
            renderer.free_mesh(ground.mesh);
        }
        if let Some(atlas) = self.atlas.take() {
            renderer.free_texture(atlas);
        }
        self.thumbs.clear();
        self.asked = None;
    }

    fn options(&self) -> PreviewOptions {
        let usual = PreviewOptions::default();
        let rotation =
            Quat::from_rotation_x(self.turn.y + TIPPED) * Quat::from_rotation_y(self.turn.x) * usual.rotation;
        PreviewOptions { size: self.size.max(64), rotation, zoom: ZOOM, ..usual }
    }

    /// Draws the ground again, turned as it now is, from `mesh` when it is a new one.
    fn redraw(&mut self, renderer: &mut Renderer, mesh: Option<MeshHandle>) {
        let Some(material) = self.material(renderer) else { return };
        let parts = mesh.map(|mesh| vec![PreviewPart { mesh, material, transform: Mat4::IDENTITY }]);
        match (self.preview, parts) {
            (Some(preview), parts) => renderer.set_preview(preview, parts, self.options()),
            (None, Some(parts)) => self.preview = Some(renderer.add_preview(parts, self.options())),
            (None, None) => {}
        }
    }

    /// The material the ground is drawn with: matt, coloured by height through its picture.
    fn material(&mut self, renderer: &mut Renderer) -> Option<MaterialHandle> {
        if self.look.is_none() {
            let rgba = ramp();
            let source =
                TextureSource { name: "ground heights", width: RAMP as u32, height: 4, rgba: &rgba, srgb: true };
            let texture = renderer.add_texture(&source).ok()?;
            let desc = MaterialDesc { roughness: 0.95, base_color_texture: Some(texture), ..MaterialDesc::default() };
            self.look = Some((texture, renderer.add_material(&desc).ok()?));
        }
        self.look.map(|(_, material)| material)
    }
}

impl Worker {
    fn start() -> Self {
        let (requests, asked) = channel::<Request>();
        let (done, made) = channel::<Made>();
        let latest = Arc::new(AtomicU64::new(0));
        let newest = latest.clone();
        let work = move || {
            while let Ok(mut request) = asked.recv() {
                // Only the newest of those that came while it was busy.
                while let Ok(newer) = asked.try_recv() {
                    request = newer;
                }
                if done.send(thumbs(&request)).is_err() {
                    return;
                }
                for across in GROUND_STEPS {
                    // Coarse for every asking, so the ground follows a drag; finer only for the last.
                    if across != GROUND_STEPS[0] && newest.load(Ordering::Relaxed) != request.generation {
                        break;
                    }
                    // A room by itself: in cells of a metre, then half a metre, and no finer.
                    let mesh = match (&request.room, across) {
                        (Some(rules), across) if across == GROUND_STEPS[0] => room(rules, request.world.seed(), 48),
                        (Some(rules), across) if across == GROUND_STEPS[3] => room(rules, request.world.seed(), 112),
                        (Some(_), _) => continue,
                        (None, across) => ground(&request.world, across),
                    };
                    if done.send(Made::Ground { generation: request.generation, across, mesh }).is_err() {
                        return;
                    }
                }
            }
        };
        if let Err(error) = std::thread::Builder::new().name("generator previews".into()).spawn(work) {
            tracing::warn!("no thread for the generator's previews: {error}");
        }
        Self { requests, made, latest }
    }
}

/// The places of a grid `across` a side over `area`, row by row from its least z: their x, and
/// their z.
fn places(area: PreviewArea, across: usize) -> (Vec<f32>, Vec<f32>) {
    let step = area.size / (across - 1).max(1) as f32;
    let (left, near) = (area.center[0] - area.size / 2.0, area.center[1] - area.size / 2.0);
    let at = |index: usize| ((index % across) as f32 * step + left, (index / across) as f32 * step + near);
    (0..across * across).map(at).unzip()
}

/// Every node's picture over the generator's preview area, in one image.
fn thumbs(request: &Request) -> Made {
    let (xs, zs) = places(request.world.preview_area(), CELL);
    let previews = request.world.previews(&xs, &zs);
    let columns = ((previews.len() as f32).sqrt().ceil() as usize).max(1);
    let rows = previews.len().div_ceil(columns).max(1);
    let pitch = CELL + GUTTER * 2;
    let (width, height) = (columns * pitch, rows * pitch);
    let mut rgba = vec![0u8; width * height * 4];
    let mut cells = Vec::with_capacity(previews.len());
    for (place, preview) in previews.iter().enumerate() {
        let finite = preview.values.iter().copied().filter(|value| value.is_finite());
        let (low, high) = finite.fold((f32::MAX, f32::MIN), |(low, high), value| (low.min(value), high.max(value)));
        let (low, high) = if low <= high { (low, high) } else { (0.0, 0.0) };
        let (left, top) = (place % columns * pitch, place / columns * pitch);
        for y in 0..pitch {
            for x in 0..pitch {
                // The gutter repeats the edge.
                let (i, j) = (x.saturating_sub(GUTTER).min(CELL - 1), y.saturating_sub(GUTTER).min(CELL - 1));
                let value = preview.values.get(j * CELL + i).copied().unwrap_or(low);
                let share = if high > low { ((value - low) / (high - low)).clamp(0.0, 1.0) } else { 0.5 };
                let grey = (share * 255.0).round() as u8;
                let pixel = ((top + y) * width + left + x) * 4;
                rgba[pixel..pixel + 4].copy_from_slice(&[grey, grey, grey, 255]);
            }
        }
        let (u, v) = ((left + GUTTER) as f32 / width as f32, (top + GUTTER) as f32 / height as f32);
        let uv = [u, v, u + CELL as f32 / width as f32, v + CELL as f32 / height as f32];
        cells.push((preview.node, preview.output, uv, [low, high]));
    }
    Made::Thumbs { generation: request.generation, size: [width as u32, height as u32], rgba, cells }
}

/// The ground `world` makes over its preview area as a block: a grid `across` places a side, its
/// middle at the origin and its heights as they are, with walls down to a flat base.
fn ground(world: &WorldGen, across: usize) -> MeshData {
    let area = world.preview_area();
    let (xs, zs) = places(area, across);
    let mut heights = vec![0.0; xs.len()];
    world.heights(&xs, &zs, &mut heights);
    for height in &mut heights {
        if !height.is_finite() {
            *height = world.sea_level();
        }
    }
    let step = area.size / (across - 1) as f32;
    let sea = world.sea_level();
    let height = |i: usize, j: usize| heights[j.min(across - 1) * across + i.min(across - 1)];
    let point =
        |i: usize, j: usize| [xs[j * across + i] - area.center[0], height(i, j), zs[j * across + i] - area.center[1]];
    let vertex = |position: [f32; 3], normal: Vec3, above: f32| Vertex {
        position,
        normal: normal.to_array(),
        uv: [shade(above), 0.5],
        tangent: [1.0, 0.0, 0.0, 1.0],
        ..Vertex::default()
    };
    let mut mesh = MeshData::default();
    for j in 0..across {
        for i in 0..across {
            // The slope from its neighbours either side, or the one there is at an edge.
            let (left, right) = (i.saturating_sub(1), (i + 1).min(across - 1));
            let (near, far) = (j.saturating_sub(1), (j + 1).min(across - 1));
            let along_x = (height(right, j) - height(left, j)) / ((right - left).max(1) as f32 * step);
            let along_z = (height(i, far) - height(i, near)) / ((far - near).max(1) as f32 * step);
            let normal = Vec3::new(-along_x, 1.0, -along_z).normalize();
            mesh.vertices.push(vertex(point(i, j), normal, height(i, j) - sea));
        }
    }
    for j in 0..across - 1 {
        for i in 0..across - 1 {
            let corner = (j * across + i) as u32;
            let (right, far) = (corner + 1, corner + across as u32);
            mesh.indices.extend([corner, far, right, right, far, far + 1]);
        }
    }
    // Its sides, down to a base a little under its lowest ground.
    let lowest = heights.iter().copied().fold(f32::MAX, f32::min);
    let base = lowest - area.size * BASE;
    let last = across - 1;
    let sides: [(Vec3, bool, Box<dyn Fn(usize) -> (usize, usize)>); 4] = [
        (Vec3::NEG_Z, false, Box::new(|k| (k, 0))),
        (Vec3::Z, true, Box::new(move |k| (k, last))),
        (Vec3::NEG_X, true, Box::new(|k| (0, k))),
        (Vec3::X, false, Box::new(move |k| (last, k))),
    ];
    for (normal, turned, along) in sides {
        let first = mesh.vertices.len() as u32;
        for k in 0..across {
            let (i, j) = along(k);
            let top = point(i, j);
            mesh.vertices.push(vertex(top, normal, top[1] - sea));
            mesh.vertices.push(vertex([top[0], base, top[2]], normal, top[1] - sea));
        }
        for k in 0..across as u32 - 1 {
            let (top, bottom, next_top, next_bottom) =
                (first + k * 2, first + k * 2 + 1, first + k * 2 + 2, first + k * 2 + 3);
            match turned {
                false => mesh.indices.extend([top, next_top, bottom, next_top, next_bottom, bottom]),
                true => mesh.indices.extend([top, bottom, next_top, next_top, bottom, next_bottom]),
            }
        }
    }
    mesh
}

/// A cave room `rules` make from `seed`, cut open: each face between a hollow cell and rock,
/// facing into the hollow, up to a fifth of the room's height over its floor and none above. So
/// it shows as its floor with a low rim of wall round it, its steps, its pillars and its ledges. In cells small enough that its longest
/// side is `across` of them, and never under a quarter of a metre.
fn room(rules: &RoomRules, seed: u32, across: usize) -> MeshData {
    let sample = rules.sample(seed);
    let [low, high] = sample.bounds();
    let longest = (0..3).map(|axis| high[axis] - low[axis]).fold(1.0, f32::max);
    let cell = (longest / across as f32).max(0.25);
    let count = [0, 1, 2].map(|axis| ((high[axis] - low[axis]) / cell).ceil() as i32 + 2);
    let middle_of = |x: i32, y: i32, z: i32| {
        Vec3::new(low[0] + (x as f32 - 0.5) * cell, low[1] + (y as f32 - 0.5) * cell, low[2] + (z as f32 - 0.5) * cell)
    };
    let at = |x: i32, y: i32, z: i32| ((y * count[2] + z) * count[0] + x) as usize;
    let mut open = vec![false; (count[0] * count[1] * count[2]) as usize];
    for y in 1..count[1] - 1 {
        for z in 1..count[2] - 1 {
            for x in 1..count[0] - 1 {
                open[at(x, y, z)] = sample.inside(middle_of(x, y, z).to_array()) > 0.0;
            }
        }
    }
    let floor = sample.floor();
    // Cut low, so its walls are a rim round its floor and none stands between the eye and the room.
    let cut = floor + ((high[1] - floor) * 0.2).max(3.0);
    let mut mesh = MeshData::default();
    // Each way out of a cell: the way, and two more with it, the first crossed with the second the way itself.
    let ways: [([i32; 3], Vec3, Vec3); 6] = [
        ([1, 0, 0], Vec3::Y, Vec3::Z),
        ([-1, 0, 0], Vec3::Z, Vec3::Y),
        ([0, 1, 0], Vec3::Z, Vec3::X),
        ([0, -1, 0], Vec3::X, Vec3::Z),
        ([0, 0, 1], Vec3::X, Vec3::Y),
        ([0, 0, -1], Vec3::Y, Vec3::X),
    ];
    for y in 1..count[1] - 1 {
        for z in 1..count[2] - 1 {
            for x in 1..count[0] - 1 {
                if !open[at(x, y, z)] {
                    continue;
                }
                let middle = middle_of(x, y, z);
                if middle.y > cut {
                    continue;
                }
                for (way, u, v) in ways {
                    if open[at(x + way[0], y + way[1], z + way[2])] {
                        continue;
                    }
                    // The face on that side, facing back into the cell.
                    let out = Vec3::new(way[0] as f32, way[1] as f32, way[2] as f32);
                    let (normal, u, v) = (-out, v * cell, u * cell);
                    let corner = middle + out * (cell * 0.5) - (u + v) * 0.5;
                    let first = mesh.vertices.len() as u32;
                    for point in [corner, corner + u, corner + u + v, corner + v] {
                        mesh.vertices.push(Vertex {
                            position: point.to_array(),
                            normal: normal.to_array(),
                            // Coloured by how high over the room's floor it is, as ground over the sea.
                            uv: [shade((point.y - floor + 1.0) * 14.0), 0.5],
                            tangent: [1.0, 0.0, 0.0, 1.0],
                            ..Vertex::default()
                        });
                    }
                    mesh.indices.extend([first, first + 1, first + 3, first + 3, first + 1, first + 2]);
                }
            }
        }
    }
    mesh
}

/// How many colours wide the ground's picture of colours by height is.
const RAMP: usize = 256;

/// Where along that picture a height above the sea (below it, under 0) takes its colour: the sea's
/// surface in the middle, heights rising to the right and depths falling to the left, both by their
/// square root, so low ground and shallows have the most colours.
fn shade(above: f32) -> f32 {
    let share = |metres: f32, most: f32| (metres / most).clamp(0.0, 1.0).sqrt() * 0.5;
    let along = if above >= 0.0 { 0.5 + share(above, HIGHEST) } else { 0.5 - share(-above, DEEPEST) };
    along.clamp(0.5 / RAMP as f32, 1.0 - 0.5 / RAMP as f32)
}

/// The ground's colours by height, as a picture [`shade`] reads across: water from deep to shallow,
/// then sand, green, rock and snow.
fn ramp() -> Vec<u8> {
    const WATER: [(f32, u32); 2] = [(0.0, 0x4f93b5), (1.0, 0x0d2b4c)];
    const LAND: [(f32, u32); 7] = [
        (0.0, 0xcbb98b),
        (0.05, 0x86a05a),
        (0.2, 0x5f8248),
        (0.4, 0x7d7458),
        (0.6, 0x8b8782),
        (0.8, 0xdde1e6),
        (1.0, 0xffffff),
    ];
    let mix = |stops: &[(f32, u32)], at: f32| -> [u8; 3] {
        let after = stops.iter().position(|(stop, _)| *stop >= at).unwrap_or(stops.len() - 1).max(1);
        let ((from, a), (to, b)) = (stops[after - 1], stops[after]);
        let share = ((at - from) / (to - from).max(1e-6)).clamp(0.0, 1.0);
        [16, 8, 0].map(|shift| {
            let (a, b) = (((a >> shift) & 0xff) as f32, ((b >> shift) & 0xff) as f32);
            (a + (b - a) * share).round() as u8
        })
    };
    let row: Vec<u8> = (0..RAMP)
        .flat_map(|column| {
            let along = (column as f32 + 0.5) / RAMP as f32;
            let [r, g, b] =
                if along < 0.5 { mix(&WATER, (0.5 - along) * 2.0) } else { mix(&LAND, (along - 0.5) * 2.0) };
            [r, g, b, 255]
        })
        .collect();
    row.repeat(4)
}

#[cfg(test)]
mod tests {
    use voxear_terrain::generate::Preset;

    use super::*;

    #[test]
    fn every_node_has_a_picture_in_one_image_with_its_least_and_most() {
        let world = Preset::RollingHills.world();
        let request = Request { generation: 1, world: world.clone(), room: None };
        let Made::Thumbs { size, rgba, cells, .. } = thumbs(&request) else {
            panic!("pictures")
        };
        // A landmass's height, and its climate's temperature and humidity: three in a square of four.
        assert_eq!(cells.len(), 3);
        let pitch = (CELL + GUTTER * 2) as u32;
        assert_eq!((size, rgba.len()), ([pitch * 2, pitch * 2], (pitch * pitch * 16) as usize));
        let (node, output, uv, [low, high]) = cells[0];
        assert_eq!((node, output), (GraphNode(1), 0));
        assert!(high - low > 1.0, "rolling hills from {low} to {high}");
        let inside = |at: f32, most: u32| (at * most as f32).round() as usize;
        assert_eq!([inside(uv[0], size[0]), inside(uv[1], size[1])], [GUTTER, GUTTER]);
        assert_eq!(inside(uv[2], size[0]) - inside(uv[0], size[0]), CELL);
        // Its least is black and its most white, somewhere in it, and its gutter repeats its edge.
        let pixel = |x: usize, y: usize| rgba[(y * size[0] as usize + x) * 4];
        let own: Vec<u8> = (0..CELL * CELL).map(|at| pixel(GUTTER + at % CELL, GUTTER + at / CELL)).collect();
        assert!(own.contains(&0) && own.contains(&255));
        assert_eq!(pixel(0, 0), pixel(GUTTER, GUTTER));
        assert_eq!(pixel(GUTTER + CELL + 1, 10), pixel(GUTTER + CELL - 1, 10));
        assert!(rgba.chunks_exact(4).take(pitch as usize).all(|pixel| pixel[3] == 255));
        // The same ground the generator makes there.
        let area = world.preview_area();
        let (xs, zs) = places(area, CELL);
        assert_eq!((xs[0], zs[0]), (-area.size / 2.0, -area.size / 2.0));
        let (last_x, last_z) = (xs[CELL * CELL - 1], zs[CELL * CELL - 1]);
        assert!((last_x - area.size / 2.0).abs() < 0.01 && (last_z - area.size / 2.0).abs() < 0.01);
        let heights: Vec<f32> = (0..xs.len()).map(|at| world.height_at(xs[at], zs[at])).collect();
        assert_eq!(heights.iter().copied().fold(f32::MAX, f32::min), low);
    }

    #[test]
    fn the_ground_is_a_block_of_the_generators_heights_facing_out() {
        let world = Preset::Alpine.world().with_preview_area(PreviewArea { center: [500.0, -200.0], size: 800.0 });
        let across = 9;
        let mesh = ground(&world, across);
        assert_eq!(mesh.vertices.len(), across * across + across * 8);
        assert_eq!(mesh.indices.len(), (across - 1) * (across - 1) * 6 + (across - 1) * 24);
        // Its middle is over the origin, at the height the generator makes there.
        let middle = mesh.vertices[(across * across) / 2];
        assert_eq!([middle.position[0], middle.position[2]], [0.0, 0.0]);
        assert_eq!(middle.position[1], world.height_at(500.0, -200.0));
        assert_eq!(mesh.vertices[0].position[0], -400.0);
        // Every triangle faces the way its corners' normals point: up for the ground, out for its sides.
        for triangle in mesh.indices.chunks_exact(3) {
            let [a, b, c] = [0, 1, 2].map(|corner| Vec3::from(mesh.vertices[triangle[corner] as usize].position));
            let facing = (b - a).cross(c - a);
            let normal = Vec3::from(mesh.vertices[triangle[0] as usize].normal);
            assert!(facing.dot(normal) > 0.0, "{triangle:?} faces {facing}, its normal {normal}");
        }
        // Heights take their colours from the sea's surface outwards.
        assert_eq!(shade(0.0), 0.5);
        assert!(shade(-1.0) < 0.5 && shade(-9999.0) > 0.0 && shade(40.0) > 0.5 && shade(9999.0) < 1.0);
        assert!(shade(100.0) - shade(0.0) > shade(2100.0) - shade(2000.0), "more colours for low ground");
        let colours = ramp();
        assert_eq!(colours.len(), RAMP * 4 * 4);
        let at = |along: f32| -> [u8; 3] {
            let pixel = ((along * RAMP as f32) as usize).min(RAMP - 1) * 4;
            [colours[pixel], colours[pixel + 1], colours[pixel + 2]]
        };
        assert!(at(0.1)[2] > at(0.1)[0], "water is blue");
        assert!(at(0.6)[1] > at(0.6)[2], "low ground is green");
        assert!(at(1.0).iter().all(|&channel| channel >= 250), "the highest ground is snow: {:?}", at(1.0));
    }
}
