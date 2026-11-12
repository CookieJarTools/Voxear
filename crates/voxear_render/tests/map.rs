//! Maps of a world of blocks drawn as small models, on the GPU: skipped
//! where there is none. Set `VOXEAR_TEST_OUTPUT` to a folder to keep the
//! pictures.

use std::sync::Arc;

use voxear_core::math::{Vec2, Vec3};
use voxear_ecs::World;
use voxear_render::{Camera, GpuMapColumn, MapCamera, MapDraw, MapId, MapLook, Renderer, RendererSettings};
use voxear_scene::Transform;
use voxear_terrain::{
    blocks::{
        chunk::BlockId,
        edits::BlockEdits,
        file::{BlockFile, BlockKind, Ground},
        generate::BlockGenerate,
        map::{MAP_TILE, MapColumn, MapGround, MapPiece, MapTile, map_tile},
        stream::BlockMaking,
    },
    generate::Preset,
};
use voxear_ui::{ImageRef, Ui};

const STONE: BlockId = 1;
const DIRT: BlockId = 2;
const GRASS: BlockId = 3;
const SEA: BlockId = 4;
const WOOD: BlockId = 5;
const LEAF: BlockId = 6;
const SAND: BlockId = 7;
const PINK_GRASS: BlockId = 8;
const PINK_LEAF: BlockId = 9;
const RED: BlockId = 10;

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;

fn renderer() -> Option<Renderer> {
    match Renderer::new_offscreen(WIDTH, HEIGHT, &RendererSettings::default()) {
        Ok(renderer) => Some(renderer),
        Err(error) => {
            eprintln!("skipping: no usable GPU ({error:#})");
            None
        }
    }
}

/// Generated ground of metre blocks, in strong colours.
fn making(preset: Preset) -> BlockMaking {
    let kind = |name: &str, color: [f32; 3]| BlockKind::colored(name, color);
    let file = BlockFile {
        block_size: 1.0,
        blocks: vec![
            kind("Stone", [0.52, 0.5, 0.5]),
            kind("Dirt", [0.62, 0.4, 0.22]),
            kind("Grass", [0.36, 0.74, 0.2]),
            BlockKind { water: true, ..kind("Sea", [0.16, 0.45, 0.9]) },
            kind("Wood", [0.45, 0.3, 0.16]),
            kind("Leaf", [0.13, 0.55, 0.16]),
            kind("Sand", [0.95, 0.82, 0.45]),
            kind("Pink Grass", [0.98, 0.55, 0.78]),
            kind("Pink Leaf", [0.8, 0.25, 0.5]),
            kind("Red", [0.95, 0.08, 0.05]),
        ],
        ground: Ground { size: [0, 0], top: GRASS, under: DIRT, under_depth: 3, bottom: STONE, ..Ground::default() },
        generate: Some(BlockGenerate {
            world: Some("hills".into()),
            caves: 0.0,
            bottom: -48,
            sea: SEA,
            ..BlockGenerate::default()
        }),
        ..BlockFile::default()
    };
    let (_, faces) = file.faces();
    let generate = file.generate.clone().unwrap();
    let world = Arc::new(preset.world());
    BlockMaking { file, generate, world, faces: faces.into(), far_folder: None }
}

/// A whole number's hash: three more out of two.
fn hash(x: i32, z: i32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x9e37_79b1) ^ (z as u32).wrapping_mul(0x85eb_ca77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^ (h >> 12)
}

/// Dresses a tile for its picture, as biomes and plants will: beaches by
/// the sea, pink country to the west, and trees, each a trunk with a crown
/// round it standing free of the ground.
fn dress(tile: &mut MapTile, sea_top: i32) {
    let side = MAP_TILE as i32;
    let corner = [tile.at[0] * side, tile.at[1] * side];
    let pink = |x: i32, z: i32| x + (((z as f32) * 0.07).sin() * 14.0) as i32 + 20 < 0;
    for z in 0..side {
        for x in 0..side {
            let (cx, cz) = (corner[0] + x, corner[1] + z);
            let column = &mut tile.columns[(z * side + x) as usize];
            if column.water > column.top {
                continue;
            }
            let land = |x: i32, z: i32| (0..side).contains(&x) && (0..side).contains(&z);
            if column.top <= sea_top + 1 {
                column.blocks[0] = SAND;
                continue;
            }
            if pink(cx, cz) {
                column.blocks[0] = PINK_GRASS;
            }
            // A tree's foot is the one cell in its 6 by 6 patch its hash picks; a crown reaches two cells from it.
            let leaf = if pink(cx, cz) { PINK_LEAF } else { LEAF };
            for (dx, dz) in (-2..=2).flat_map(|dz| (-2..=2).map(move |dx| (dx, dz))) {
                let (tx, tz) = (cx + dx, cz + dz);
                let patch = hash(tx.div_euclid(6), tz.div_euclid(6));
                let foot = [(patch % 6) as i32, ((patch >> 8) % 6) as i32];
                // Only trees whose foot is in this tile, so none stands with its trunk in the sea next door.
                if patch >> 16 & 3 == 0 || [tx.rem_euclid(6), tz.rem_euclid(6)] != foot || !land(x + dx, z + dz) {
                    continue;
                }
                let tall = 3 + (patch >> 20 & 3) as i32;
                if (dx, dz) == (0, 0) {
                    // The trunk's own column: wood, then leaves over it.
                    column.pieces[0] = MapPiece { from: column.top, to: column.top + tall, top: WOOD, side: WOOD };
                    column.pieces[1] =
                        MapPiece { from: column.top + tall, to: column.top + tall + 3, top: leaf, side: leaf };
                } else if dx * dx + dz * dz <= 4 {
                    let low = column.top + tall + i32::from(dx.abs() + dz.abs() > 2);
                    let high = column.top + tall + 3 - i32::from(dx.abs() + dz.abs() > 1);
                    column.pieces[1] = MapPiece { from: low.min(high - 1), to: high, top: leaf, side: leaf };
                }
            }
        }
    }
}

/// A map of `preset`'s ground, `side` cells a side at `level`, round the
/// tile `middle`, every tile written: the map, its tiles, and its window's
/// lowest cell.
fn ground(renderer: &mut Renderer, preset: Preset, (side, level): (u32, u32), middle: [i32; 2]) -> Scene {
    let making = making(preset);
    let colors: Vec<[u8; 3]> = making.faces.iter().map(|faces| faces.color).collect();
    let sea_top = (making.world.sea_level() / (1 << level) as f32).floor() as i32;
    let ground = MapGround::made(making, Arc::new(BlockEdits::default()));
    let map = renderer.add_map(side).unwrap();
    renderer.set_map_palette(map, &colors).unwrap();
    let half = (side / MAP_TILE as u32) as i32 / 2;
    let places: Vec<[i32; 2]> =
        (-half..half).flat_map(|z| (-half..half).map(move |x| [middle[0] + x, middle[1] + z])).collect();
    let tiles = voxear_terrain::workers::each(&places, |at| {
        let mut tile = map_tile(&ground, level, *at);
        dress(&mut tile, sea_top);
        tile
    });
    for tile in &tiles {
        let columns: Vec<GpuMapColumn> = tile.columns.iter().map(GpuMapColumn::of).collect();
        renderer.write_map_tile(map, tile.at.map(|at| at * MAP_TILE as i32), &columns);
    }
    let corner = middle.map(|at| (at - half) * MAP_TILE as i32);
    Scene { map, tiles, corner, side }
}

/// A map with its tiles written.
struct Scene {
    map: MapId,
    tiles: Vec<MapTile>,
    corner: [i32; 2],
    side: u32,
}

/// The tile of `preset`'s ground at `level` nearest the origin where a hill
/// stands by the sea.
fn coast(preset: Preset, level: u32) -> [i32; 2] {
    let world = preset.world();
    let sea = world.sea_level();
    let tile = (MAP_TILE as i32 * (1 << level)) as f32;
    let shore = |x: f32, z: f32| world.height_at(x, z) > sea + 10.0 && world.height_at(x + tile * 1.5, z) < sea - 2.0;
    let found =
        (0..80).flat_map(|ring: i32| (-ring..=ring).flat_map(move |z| [(-ring, z), (ring, z), (z, -ring), (z, ring)]));
    let found = found.map(|(x, z)| [x, z]).find(|[x, z]| shore(*x as f32 * tile, *z as f32 * tile));
    found.expect("a coast within eighty tiles")
}

/// The lowest ground and the highest block of a scene's columns within
/// `reach` cells of its window's middle.
fn heights(scene: &Scene, reach: f32) -> [i32; 2] {
    let (side, half) = (MAP_TILE as i32, scene.side as i32 / 2);
    let middle = [scene.corner[0] + half, scene.corner[1] + half];
    let mut range = [i32::MAX, i32::MIN];
    for tile in &scene.tiles {
        for (at, column) in tile.columns.iter().enumerate().filter(|(_, column)| column.is_some()) {
            let cell = [tile.at[0] * side + at as i32 % side, tile.at[1] * side + at as i32 / side];
            let off = [cell[0] - middle[0], cell[1] - middle[1]].map(|v| v.abs() as f32);
            if off[0] <= reach && off[1] <= reach {
                range = [range[0].min(column.top), range[1].max(column.highest())];
            }
        }
    }
    range
}

/// The column at cell `[x, z]` of `tiles`.
fn column(tiles: &[MapTile], [x, z]: [i32; 2]) -> MapColumn {
    let side = MAP_TILE as i32;
    let tile = tiles.iter().find(|tile| tile.at == [x.div_euclid(side), z.div_euclid(side)]).unwrap();
    *tile.column([x.rem_euclid(side) as u32, z.rem_euclid(side) as u32])
}

/// Renders the map as `draw` says, shown over the whole output on a plain
/// blue backdrop, and reads the output back.
fn picture(renderer: &mut Renderer, map: MapId, draw: MapDraw) -> Vec<u8> {
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::IDENTITY));
    renderer.draw_map(map, draw);
    renderer.render(&world, 0.1).unwrap();
    let image = renderer.map_image(map).expect("drawn with the frame");
    assert_eq!(image.size, draw.size);
    let mut ui = Ui::new();
    let root = ui.root();
    let backdrop = ui.panel(root, voxear_ui::Style::default());
    let whole = "position: absolute; left: 0px; top: 0px; width: 100%; height: 100%";
    ui.set_inline_style(backdrop, &format!("{whole}; background-color: #1c2430"));
    let shown = ui.image(root);
    ui.set_inline_style(shown, whole);
    ui.set_image(shown, Some(ImageRef { texture: image.texture.0, uv: [0.0, 0.0, 1.0, 1.0] }));
    renderer.set_ui(&ui.frame(WIDTH, HEIGHT, 1.0)).unwrap();
    for frame in 0..3 {
        renderer.draw_map(map, draw);
        renderer.render(&world, 0.2 + frame as f32 * 0.1).unwrap();
    }
    renderer.read_pixels().unwrap().2
}

fn keep(name: &str, pixels: &[u8]) {
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
        let mut encoder = png::Encoder::new(file, WIDTH, HEIGHT);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(pixels).unwrap();
    }
}

fn pixel(pixels: &[u8], at: Vec2) -> [u8; 4] {
    let i = ((at.y as u32 * WIDTH + at.x as u32) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

/// A slab of a scene's ground reaching `reach` cells from its window's
/// middle, seen turned and tilted, as the big map shows it.
fn slab(scene: &Scene, reach: f32, (turn, tilt): (f32, f32), round: bool) -> MapDraw {
    let [low, high] = heights(scene, reach);
    let half = scene.side as f32 * 0.5;
    let fov = 0.4_f32;
    let camera = MapCamera {
        middle: Vec3::new(half, (high - low) as f32 * 0.4, half),
        turn,
        tilt,
        distance: reach * 1.45 / (fov * 0.5).tan(),
        fov,
    };
    MapDraw {
        size: [WIDTH, HEIGHT],
        corner: scene.corner,
        base: low,
        camera,
        reach,
        round,
        heights: [0.0, (high - low) as f32],
        thick: 6.0,
        samples: 3,
        look: MapLook::default(),
    }
}

#[test]
fn a_map_draws_a_slab_of_the_world_as_a_model_with_its_cut_sides_and_nothing_round_it() {
    let Some(mut renderer) = renderer() else { return };
    let Scene { map, mut tiles, corner, side: cells } = ground(&mut renderer, Preset::RollingHills, (256, 1), [0, 0]);
    // A red tower at the slab's middle, to find in the picture.
    let side = MAP_TILE as i32;
    let tile = tiles.iter_mut().find(|tile| tile.at == [0, 0]).unwrap();
    let top = tile.columns[0].top.max(tile.columns[0].water);
    for z in 0..3 {
        for x in 0..3 {
            let column = &mut tile.columns[(z * side + x) as usize];
            *column = MapColumn { top: top + 12, blocks: [RED; 3], depths: [255, 0], water: top + 12, ..*column };
            column.pieces = [MapPiece::NONE; 2];
        }
    }
    let columns: Vec<GpuMapColumn> = tile.columns.iter().map(GpuMapColumn::of).collect();
    renderer.write_map_tile(map, [0, 0], &columns);

    let scene = Scene { map, tiles, corner, side: cells };
    let tiles = &scene.tiles;
    let draw = slab(&scene, 120.0, (0.6, 0.72), false);
    let pixels = picture(&mut renderer, map, draw);
    keep("map-slab.png", &pixels);

    // The tower's top shows where the camera says it does, in its own colour.
    let size = Vec2::new(WIDTH as f32, HEIGHT as f32);
    let tower = Vec3::new(128.0 + 1.5, (top + 12 - draw.base) as f32, 128.0 + 1.5);
    let at = draw.camera.project(tower, size).unwrap();
    let seen = pixel(&pixels, at);
    assert!(seen[0] > 150 && seen[1] < 80 && seen[2] < 80, "the red tower's top at {at:?}, got {seen:?}");

    // Nothing is drawn past the slab: the corners show what is behind the UI's picture.
    let plain = pixel(&pixels, Vec2::new(4.0, 4.0));
    assert_eq!([plain[0], plain[1], plain[2]], [0x1c, 0x24, 0x30], "the backdrop shows where the map draws nothing");
    assert_eq!(plain, pixel(&pixels, Vec2::new(WIDTH as f32 - 5.0, HEIGHT as f32 - 5.0)));
    assert_ne!([seen[0], seen[1], seen[2]], [plain[0], plain[1], plain[2]]);

    // Ground stands where the camera says: a column's top, a few cells from the middle, is not the backdrop.
    let near = column(tiles, [20, -30]);
    let ground = Vec3::new(128.0 + 20.5, (near.highest() - draw.base) as f32, 128.0 - 29.5);
    let ground = pixel(&pixels, draw.camera.project(ground, size).unwrap());
    assert_ne!(ground, plain, "the slab covers its middle");

    renderer.free_map(map);
}

#[test]
fn a_round_map_seen_from_above_is_a_minimap() {
    let Some(mut renderer) = renderer() else { return };
    let scene = ground(&mut renderer, Preset::RollingHills, (128, 2), [0, 0]);
    let map = scene.map;
    let mut draw = slab(&scene, 56.0, (0.0, std::f32::consts::FRAC_PI_2), true);
    draw.camera.distance *= 0.75;
    let pixels = picture(&mut renderer, map, draw);
    keep("map-round.png", &pixels);
    let size = Vec2::new(WIDTH as f32, HEIGHT as f32);
    let plain = pixel(&pixels, Vec2::new(4.0, 4.0));
    // Seen from straight above, north is up: a cell north of the middle shows over it, and the round slab ends
    // inside the square it was cut from.
    let middle = draw.camera.project(draw.camera.middle, size).unwrap();
    assert!((middle - size * 0.5).length() < 1.0);
    assert_ne!(pixel(&pixels, middle), plain);
    let corner = draw.camera.project(draw.camera.middle + Vec3::new(draw.reach * 0.9, 0.0, draw.reach * 0.9), size);
    assert_eq!(pixel(&pixels, corner.unwrap()), plain, "past the round slab's edge");
    let edge = draw.camera.project(draw.camera.middle + Vec3::new(draw.reach * 0.6, 0.0, draw.reach * 0.6), size);
    assert_ne!(pixel(&pixels, edge.unwrap()), plain, "inside it");
    renderer.free_map(map);
}

/// The least of `frames` frames' time in the map's pass, in milliseconds,
/// the map turned a little each frame so each draws it anew.
fn least_pass_ms(renderer: &mut Renderer, map: MapId, draw: MapDraw, frames: u32) -> f32 {
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::IDENTITY));
    let mut least = f32::MAX;
    for frame in 0..frames {
        let camera = MapCamera { turn: draw.camera.turn + frame as f32 * 0.002, ..draw.camera };
        renderer.draw_map(map, MapDraw { camera, ..draw });
        renderer.render(&world, frame as f32 * 0.016).unwrap();
        // Timings come back a few frames late: the first are the last thing drawn.
        let pass = renderer.timings().iter().find(|(name, _)| name == "Map").map(|(_, ms)| *ms);
        least = least.min(pass.filter(|ms| *ms > 0.0 && frame >= 6).unwrap_or(f32::MAX));
    }
    least
}

#[test]
fn mountains_close_up_show_cliffs_shores_and_trees_as_blocks() {
    let Some(mut renderer) = renderer() else { return };
    let scene = ground(&mut renderer, Preset::Alpine, (128, 1), coast(Preset::Alpine, 1));
    let map = scene.map;
    let mut draw = slab(&scene, 44.0, (-1.9, 0.62), false);
    draw.thick = 3.0;
    let pixels = picture(&mut renderer, map, draw);
    keep("map-close.png", &pixels);
    // Sea, sand, grass and leaves all show: each colour is found somewhere in the picture.
    let has = |want: fn([u8; 4]) -> bool| pixels.chunks_exact(4).any(|p| want([p[0], p[1], p[2], p[3]]));
    assert!(has(|[r, g, b, _]| b > 150 && b > r + 60 && g < b && [r, g, b] != [0x1c, 0x24, 0x30]), "sea");
    assert!(has(|[r, g, b, _]| g > 120 && g > r + 40 && g > b + 60), "grass or leaves");
    renderer.free_map(map);
}

#[test]
#[ignore = "prints how long the map's pass takes; run by hand"]
fn map_time() {
    let Some(mut renderer) = renderer() else { return };
    let small = ground(&mut renderer, Preset::Alpine, (128, 1), coast(Preset::Alpine, 1));
    let mut mini = slab(&small, 56.0, (0.3, 1.1), true);
    (mini.size, mini.samples) = ([256, 256], 2);
    println!("minimap 256 px, 4 rays a pixel: {:.3} ms", least_pass_ms(&mut renderer, small.map, mini, 66));
    let large = ground(&mut renderer, Preset::Alpine, (512, 1), coast(Preset::Alpine, 1));
    for (samples, shadows) in [(1, 0.4), (1, 0.0), (2, 0.4), (3, 0.4)] {
        let mut big = slab(&large, 248.0, (0.6, 0.72), false);
        big.camera.distance *= 0.62;
        (big.size, big.samples, big.look.shadows) = ([2560, 1440], samples, shadows);
        let ms = least_pass_ms(&mut renderer, large.map, big, 66);
        println!("big map 2560x1440, {} rays a pixel, shadows {shadows}: {ms:.3} ms", samples * samples);
    }
}
