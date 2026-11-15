//! Dungeons of blocks drawn on a real GPU, offscreen: the entrance in the
//! land and the spaces inside, each lit by its braziers and crystals. Each
//! test skips itself (with a note) on machines without a usable Vulkan
//! device. Set `VOXEAR_TEST_OUTPUT` to a folder to keep the pictures.

use std::sync::Arc;

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::World;
use voxear_render::{
    AntiAliasing, AntiAliasingMode, BlockLook, Camera, Fog, Light, MaterialDesc, PostProcess, Renderer,
    RendererSettings, Sky, SurfaceLook,
};
use voxear_scene::Transform;
use voxear_terrain::{
    blocks::{
        BlockWorld,
        block::BlockShape,
        dungeon::{self, Dungeon, DungeonBlocks, DungeonLayout, EntranceKind, LightKind, SpaceKind, WayKind},
        file::{BlockFile, BlockKind, Ground},
        generate::BlockGenerate,
        structure::{GroundFit, PlacedStructure, StructureBlocks, WorldStructures},
    },
    generate::{Preset, WorldGen, structures::Placement},
};

const WIDTH: u32 = 960;
const HEIGHT: u32 = 540;

fn renderer() -> Option<Renderer> {
    match Renderer::new_offscreen(WIDTH, HEIGHT, &RendererSettings::default()) {
        Ok(renderer) => Some(renderer),
        Err(error) => {
            eprintln!("skipping: no usable GPU ({error:#})");
            None
        }
    }
}

fn render(renderer: &mut Renderer, world: &World, frames: u32) -> Vec<u8> {
    let mut time = 0.0;
    for _ in 0..frames {
        time += 1.0 / 60.0;
        renderer.render(world, time).expect("frame renders");
    }
    renderer.read_pixels().expect("pixels").2
}

fn keep_image(name: &str, pixels: &[u8]) {
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
        let mut encoder = png::Encoder::new(file, WIDTH, HEIGHT);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(pixels).unwrap();
    }
}

/// The theme's blocks, after the world's own three: each a name, a colour,
/// what it glows, and whether it is water, a slab or a vine.
const BLOCKS: [(&str, [f32; 3], [f32; 3]); 15] = [
    ("Stone", [0.42, 0.40, 0.37], [0.0; 3]),
    ("Trim", [0.62, 0.58, 0.50], [0.0; 3]),
    ("Flagstones", [0.50, 0.47, 0.42], [0.0; 3]),
    ("Rubble", [0.38, 0.36, 0.33], [0.0; 3]),
    ("Cracked", [0.34, 0.33, 0.31], [0.0; 3]),
    ("Mossy", [0.30, 0.40, 0.22], [0.0; 3]),
    ("Slab", [0.50, 0.47, 0.42], [0.0; 3]),
    ("Stairs", [0.50, 0.47, 0.42], [0.0; 3]),
    ("Statue", [0.60, 0.58, 0.54], [0.0; 3]),
    ("Iron", [0.12, 0.12, 0.13], [0.0; 3]),
    ("Fire", [1.0, 0.55, 0.2], [6.0, 2.6, 0.7]),
    ("Crystal", [0.4, 0.7, 1.0], [0.6, 1.6, 3.2]),
    ("Water", [0.12, 0.32, 0.26], [0.0; 3]),
    ("Lava", [1.0, 0.3, 0.05], [5.0, 1.2, 0.2]),
    ("Vine", [0.22, 0.38, 0.16], [0.0; 3]),
];

/// A temple dungeon of the theme's blocks, numbered from `first`, with
/// only `entrances`.
fn temple(first: u16, entrances: Vec<EntranceKind>) -> Dungeon {
    let at = |i: u16| first + i;
    Dungeon {
        blocks: DungeonBlocks {
            stone: at(0),
            trim: at(1),
            floor: at(2),
            rubble: at(3),
            cracked: at(4),
            mossy: at(5),
            slab: at(6),
            stairs: [at(7); 4],
            statue: at(8),
            metal: at(9),
            light: at(10),
            cold_light: at(11),
            water: at(12),
            lava: at(13),
            vine: at(14),
        },
        floors: [2, 2],
        entrances,
        ..Dungeon::default()
    }
}

/// A world of blocks holding the whole of a dungeon with only `entrances`
/// in `world`, its layout, and the dungeon's first block's number.
fn dungeon_world(world: &WorldGen, entrances: Vec<EntranceKind>) -> Option<(BlockFile, Arc<DungeonLayout>)> {
    let mut file = BlockFile { block_size: 1.0, ..BlockFile::default() };
    let first = file.blocks.len() as u16 + 1;
    for (name, color, _) in BLOCKS {
        let mut kind = BlockKind::colored(name, color);
        kind.water = name == "Water";
        kind.slab = name == "Slab";
        if name == "Vine" {
            kind.shape = BlockShape::Vine;
        }
        file.blocks.push(kind);
    }
    let dungeon = temple(first, entrances);
    let placement = Placement { spacing: 500.0, separation: 100.0, chance: 1.0, slope: 80.0, ..Placement::default() };
    for site in placement.tries_near(world.seed(), [-3000.0; 2], [3000.0; 2], 0.0) {
        if world.height_at(site.at[0], site.at[1]) < world.sea_level() + 4.0 {
            continue;
        }
        // First where its blocks' corner is a way off the site, to find how far it reaches; then the world just
        // holding it, moved by whole readings of the land so it plans the same.
        let guess = [(site.at[0] / 4.0).floor() * 4.0 - 400.0, (site.at[1] / 4.0).floor() * 4.0 - 400.0];
        let Some(layout) = dungeon::layout_at(&dungeon, world, (1.0, guess, -400, false), &site) else { continue };
        let low = [(layout.low[0] - 12).div_euclid(4) * 4, (layout.low[2] - 12).div_euclid(4) * 4];
        let size = [(layout.high[0] + 12 - low[0]) as u32, (layout.high[2] + 12 - low[1]) as u32];
        if size[0] > 720 || size[1] > 720 {
            continue;
        }
        let position = [guess[0] + low[0] as f32, guess[1] + low[1] as f32];
        let bottom = layout.low[1] - 8;
        let layout = dungeon::layout_at(&dungeon, world, (1.0, position, bottom, false), &site)?;
        file.ground = Ground { size, ..Ground::default() };
        let list = vec![PlacedStructure {
            placement: placement.clone(),
            ground: GroundFit::Leave,
            sink: 0,
            blocks: StructureBlocks::Dungeon(dungeon.clone()),
            biomes: Vec::new(),
        }];
        file.generate = Some(BlockGenerate {
            caves: 0.0,
            bottom,
            position,
            built: Some(Arc::new(WorldStructures { list, ..WorldStructures::default() })),
            ..BlockGenerate::default()
        });
        return Some((file, layout));
    }
    None
}

/// A world whose land climbs in escarpments: areas 160 m across, from 4 m
/// up to 160 m in four levels, walls between.
fn escarpments() -> Arc<WorldGen> {
    use voxear_terrain::generate::{BiomeBlocks, BiomeDef, Biomes, LevelPlan, Regions, WorldBiomes, nodes::Levels};
    let world = Arc::new(Preset::RollingHills.world());
    let levels = Levels { size: 160.0, levels: 4, low: 4.0, high: 160.0, peaks: 0.0, ..Levels::default() };
    let list = vec![BiomeDef { temperature: [-60.0, 60.0], humidity: [0.0, 1.0], ..BiomeDef::default() }];
    let biomes = Biomes::new(list, 15.0, 0.3, world.seed());
    let regions = Regions::new(160.0, 16.0, biomes.clone(), world.clone());
    let regions = Arc::new(regions.with_levels(LevelPlan { keep: None, ..levels.plan() }));
    let generate = BlockGenerate {
        levels: Some(levels),
        placed: Some(Arc::new(WorldBiomes {
            regions: Some(regions),
            biomes,
            blocks: vec![BiomeBlocks { top: 1, ..Default::default() }],
        })),
        ..Default::default()
    };
    voxear_terrain::blocks::generate::landed(&generate, &world)
}

/// A rotation that looks from `from` towards `to`: cameras look along +z.
fn looking(from: Vec3, to: Vec3) -> Quat {
    let d = (to - from).normalize();
    let yaw = d.x.atan2(d.z);
    let pitch = (-d.y).asin();
    Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch)
}

/// The scene of `file`'s blocks, each lit as its colour, the glowing ones
/// glowing; lamps at the layout's lights, a low sun, dusty air.
fn scene(
    renderer: &mut Renderer,
    file: &BlockFile,
    map: &voxear_terrain::blocks::BlockMap,
    layout: &DungeonLayout,
) -> World {
    let (looks, faces) = file.faces();
    let surfaces: Vec<SurfaceLook> = looks
        .iter()
        .map(|look| {
            let [r, g, b] = look.color;
            let glow = BLOCKS.iter().find(|(_, color, _)| *color == look.color).map_or([0.0; 3], |(_, _, glow)| *glow);
            let material = renderer
                .add_material(&MaterialDesc {
                    base_color: [r, g, b, 1.0],
                    emissive: glow,
                    roughness: 0.85,
                    ..MaterialDesc::default()
                })
                .unwrap();
            SurfaceLook {
                material,
                size: 1.0,
                sharpness: 0.0,
                triplanar: true,
                tracks: 0.0,
                fills_in: 0.0,
                liquid: Default::default(),
                glitter: false,
                leafy: false,
                mixed: false,
                tint: 0,
            }
        })
        .collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let mut scene = World::new();
    let look = BlockLook {
        material: default,
        surfaces: Arc::from(surfaces),
        voxels: Default::default(),
        motions: Arc::from([]),
    };
    scene.spawn((BlockWorld::made(map.clone(), &faces, 1.0, false), look, Transform::default()));
    for light in &layout.lights {
        let p = Vec3::new(light.at[0] as f32 + 0.5, light.at[1] as f32 + 1.8, light.at[2] as f32 + 0.5);
        let lamp = match light.kind {
            LightKind::Brazier => Light::point(Vec3::new(1.0, 0.55, 0.22), 30.0, 24.0),
            LightKind::Crystal => Light::point(Vec3::new(0.35, 0.65, 1.0), 16.0, 18.0),
            LightKind::Glow => Light::point(Vec3::new(1.0, 0.35, 0.08), 120.0, 48.0),
        };
        scene.spawn((lamp, Transform::from_position(p)));
    }
    // The sun behind whoever looks at the entrance, to one side.
    let out = [-layout.entrance.into[0] as f32, -layout.entrance.into[1] as f32];
    let towards_sun = Vec3::new(out[0] * 0.6 - out[1] * 0.4, 0.75, out[1] * 0.6 + out[0] * 0.4).normalize();
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    scene.spawn((Light::directional(Vec3::new(1.0, 0.93, 0.82), 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    scene.insert_resource(Sky { ambient: 0.1, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    let mut post = PostProcess { anti_aliasing, ..PostProcess::default() };
    post.fog =
        Fog { enabled: true, density: 0.004, height_falloff: 0.0001, color: Some([0.3, 0.27, 0.24]), ..post.fog };
    scene.insert_resource(post);
    scene
}

/// Draws the camera at `from` looking at `to` and keeps it as `name`.
fn shot(renderer: &mut Renderer, scene: &mut World, name: &str, from: Vec3, to: Vec3) {
    let camera = scene.spawn((
        Camera { fov_y_degrees: 62.0, ..Camera::default() },
        Transform::new(from, looking(from, to), Vec3::ONE),
    ));
    let pixels = render(renderer, scene, 40);
    keep_image(name, &pixels);
    scene.despawn(camera);
}

/// The middle of a block, `up` over it.
fn over(at: [i32; 3], up: f32) -> Vec3 {
    Vec3::new(at[0] as f32 + 0.5, at[1] as f32 + up, at[2] as f32 + 0.5)
}

/// Where to stand to see `space`, and what to look at.
fn viewpoint(layout: &DungeonLayout, space: &dungeon::Space) -> (Vec3, Vec3) {
    let c = Vec3::new(space.at[0] as f32 + 0.5, space.at[1] as f32 + 1.0, space.at[2] as f32 + 0.5);
    let (hx, hz) = (space.half[0] as f32, space.half[1] as f32);
    let long = if space.along_x { Vec3::new(hx, 0.0, 0.0) } else { Vec3::new(0.0, 0.0, hz) };
    match space.kind {
        SpaceKind::Hall => (
            c - long * 0.85 + Vec3::new(0.0, 5.0, 0.0),
            c + long * 0.8 + Vec3::new(0.0, space.height as f32 * 0.3, 0.0),
        ),
        SpaceKind::Flooded => {
            // From a walkway at one end, across the water.
            let across = if space.along_x { Vec3::new(0.0, 0.0, hz) } else { Vec3::new(hx, 0.0, 0.0) };
            (
                c - long * 0.85 + across * 0.75 + Vec3::new(0.0, 4.0, 0.0),
                c + long * 0.6 - across * 0.2 + Vec3::new(0.0, 4.0, 0.0),
            )
        }
        SpaceKind::Gorge => {
            // From the terrace at its upper way in, down along it.
            let upper = layout.ways.iter().flat_map(|way| [way.line[0], *way.line.last().unwrap()]).find(|p| {
                (p[2] as i32).div_euclid(2) - 1 == space.upper
                    && (p[0] - c.x).abs() <= hx + 3.0
                    && (p[1] - c.z).abs() <= hz + 3.0
            });
            let near =
                upper.map_or(1.0, |p| if (Vec3::new(p[0], 0.0, p[1]) - c).dot(long) >= 0.0 { 1.0 } else { -1.0 });
            let top = space.upper as f32 + 5.0;
            (Vec3::new(c.x, top, c.z) + long * near * 1.05, c - long * near * 0.6 - Vec3::new(0.0, 2.0, 0.0))
        }
        SpaceKind::Chasm => (c + Vec3::new(-hx * 0.8, 4.0, -hz * 0.25), c + Vec3::new(0.0, 4.0, 0.0)),
        SpaceKind::Descent => {
            (Vec3::new(c.x - hx * 0.6, space.upper as f32 + 4.0, c.z - hz * 0.3), c + Vec3::new(hx * 0.3, -2.0, 0.0))
        }
        SpaceKind::Arena => {
            (c + Vec3::new(0.0, space.height as f32 * 0.5, 0.0) - long * 0.2, c + Vec3::new(hx * 0.5, 0.0, 0.0))
        }
        _ => (c + Vec3::new(-hx * 0.7, 4.0, -hz * 0.7), c + Vec3::new(hx * 0.6, 2.0, hz * 0.6)),
    }
}

#[test]
fn a_temple_dungeon_is_drawn_from_its_entrance_in_the_land_down_through_its_spaces() {
    let Some(mut renderer) = renderer() else { return };
    let world = match std::env::var("VOXEAR_DUNGEON_WORLD").as_deref() {
        Ok("escarpments") => (*escarpments()).clone(),
        _ => Preset::RollingHills.world(),
    };
    let kinds: Vec<EntranceKind> = match std::env::var("VOXEAR_DUNGEON_ENTRANCE").as_deref() {
        Ok("cliff") => vec![EntranceKind::Cliff],
        Ok("temple") => vec![EntranceKind::Temple],
        Ok("sinkhole") => vec![EntranceKind::Sinkhole],
        _ => Vec::new(),
    };
    let (file, layout) = dungeon_world(&world, kinds).expect("a dungeon in rolling hills");
    let map = file.make_ground(Some(&world));
    let mut scene = scene(&mut renderer, &file, &map, &layout);
    let kind = format!("{:?}", layout.entrance.kind).to_lowercase();
    // The entrance, from out on the land before it.
    let e = layout.entrance;
    let out = [-e.into[0] as f32, -e.into[1] as f32];
    let side = [-out[1], out[0]];
    let (back, up) = if layout.entrance.kind == EntranceKind::Sinkhole { (22.0, 20.0) } else { (34.0, 6.0) };
    let target = over(e.at, if layout.entrance.kind == EntranceKind::Sinkhole { -6.0 } else { 7.0 });
    let from = over(e.at, 7.0) + Vec3::new(out[0] * back + side[0] * 10.0, up, out[1] * back + side[1] * 10.0);
    scene.insert_resource(Sky { ambient: 0.35, ..Sky::default() });
    shot(&mut renderer, &mut scene, &format!("dungeon entrance {kind}.png"), from, target);
    scene.insert_resource(Sky { ambient: 0.1, ..Sky::default() });
    // Down the grand stair from its top.
    if let Some(grand) = layout.ways.iter().find(|way| way.kind == WayKind::Grand) {
        let (top, dir) = grand.point(2.0);
        let (down, _) = grand.point(grand.length * 0.6);
        let from = Vec3::new(top[0], top[2] * 0.5 + 3.5, top[1]);
        let to = Vec3::new(down[0], down[2] * 0.5 + 1.0, down[1]);
        let _ = dir;
        shot(&mut renderer, &mut scene, "dungeon grand stair.png", from, to);
    }
    // Each kind of space once, from where it shows best.
    let mut seen = std::collections::BTreeSet::new();
    for space in layout.spaces.iter() {
        if space.height < 0 || !seen.insert(format!("{:?}", space.kind)) {
            continue;
        }
        let (from, to) = viewpoint(&layout, space);
        shot(
            &mut renderer,
            &mut scene,
            &format!("dungeon {}.png", format!("{:?}", space.kind).to_lowercase()),
            from,
            to,
        );
    }
}
