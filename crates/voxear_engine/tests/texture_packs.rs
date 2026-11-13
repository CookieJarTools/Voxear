//! Texture packs on a Block World: a pack worn gives the blocks it names
//! faces of its own and a material it names other pictures, a world takes
//! it up as it is worn and lets go of it as it is taken off, and it is
//! drawn so: the game's pictures, a player's own files, and a grain made of
//! each block's colour. And faces that join: panes of glass, by their
//! material, and a block a pack gives joining tiles. And blocks that meet:
//! grass creeping over the edge of dirt and stone beside it.

use voxear_ecs::Entity;
use voxear_engine::{Engine, Game, Project, blocks::BlockWorldAsset, texture_packs};
use voxear_terrain::blocks::{
    BlockWorld,
    block::BlockDef,
    file::{BlockFile, BlockKind},
};

const SCENE: &str = "format = 1\n\n[[obj]]\nid = 1\nname = \"Blocks\"\nposition = [-16.0, 0.0, -16.0]\n\
                     [obj.block_world]\ndata = \"Blocks/Field.blockworld\"\n\n\
                     [[obj]]\nid = 2\nname = \"Eye\"\nposition = [0.6, 9.4, -8.5]\nrotation = [22.0, 0.0, 0.0]\n\
                     [obj.camera]\nfov = 50.0\n\n\
                     [[obj]]\nid = 3\nname = \"Sun\"\nrotation = [50.0, 30.0, 0.0]\n[obj.light]\n\
                     type = \"directional\"\n";

/// A picture of `side` pixels a side, each pixel what `pixel` says for it.
fn picture(file: &std::path::Path, side: u32, pixel: impl Fn(u32, u32) -> [u8; 3]) {
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let mut pixels = Vec::new();
    for y in 0..side {
        for x in 0..side {
            pixels.extend(pixel(x, y));
            pixels.push(255);
        }
    }
    let mut encoder = png::Encoder::new(std::fs::File::create(file).unwrap(), side, side);
    encoder.set_color(png::ColorType::Rgba);
    encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
}

/// A number from 0 to 1 for a pixel, the same every time.
fn speck(x: u32, y: u32, seed: u32) -> f32 {
    let mut h = x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263) ^ seed.wrapping_mul(2_246_822_519);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    ((h ^ (h >> 16)) & 0xFFFF) as f32 / 65535.0
}

fn shaded(color: [u8; 3], by: f32) -> [u8; 3] {
    color.map(|part| (f32::from(part) * by).clamp(0.0, 255.0) as u8)
}

/// A flat world of grass over dirt over stone, each a block of a colour alone, with planks, a block with a
/// material and a picture of its own; a pack of the game's with pixel pictures for all four; and a camera looking
/// down at where [`build`] puts a few of each.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let mut file = BlockFile::default();
    (file.ground.size, file.ground.height) = ([32, 32], 6);
    file.blocks[0] = BlockKind::of("Blocks/Grass.block");
    file.blocks[1] = BlockKind::of("Blocks/Dirt.block");
    file.blocks[2] = BlockKind::of("Blocks/Stone.block");
    file.blocks.push(BlockKind::of("Blocks/Planks.block"));
    let planks =
        BlockDef { material: Some("Materials/Planks.material".into()), ..BlockDef::colored([0.6, 0.45, 0.25]) };
    let pack = "format = 1\ndescription = \"Sixteen pixels a side.\"\npixels = true\n\n\
                [[block]]\nblock = \"Blocks/Grass.block\"\ntop = \"Packs/Pixel/grass_top.png\"\n\
                sides = \"Packs/Pixel/grass_side.png\"\nbottom = \"Packs/Pixel/dirt.png\"\n\n\
                [[block]]\nblock = \"Blocks/Dirt.block\"\nall = \"Packs/Pixel/dirt.png\"\n\n\
                [[block]]\nblock = \"Blocks/Stone.block\"\nall = { color_map = \"Packs/Pixel/bricks.png\" }\n\n\
                [[material]]\nmaterial = \"Materials/Planks.material\"\ncolor_map = \"Packs/Pixel/planks.png\"\n";
    let files = [
        ("voxear.toml", "[project]\nname = \"Packs\"\nstart_level = \"Levels/Main.level\"\n".to_string()),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Field.scene\"\n".to_string()),
        ("Scenes/Field.scene", SCENE.to_string()),
        ("Blocks/Field.blockworld", file.to_text()),
        ("Blocks/Grass.block", BlockDef::colored([0.25, 0.55, 0.15]).to_text()),
        ("Blocks/Dirt.block", BlockDef::colored([0.42, 0.3, 0.2]).to_text()),
        ("Blocks/Stone.block", BlockDef::colored([0.5, 0.5, 0.52]).to_text()),
        ("Blocks/Planks.block", planks.to_text()),
        ("Materials/Planks.material", "color = [1.0, 1.0, 1.0]\nroughness = 0.9\n".to_string()),
        ("Packs/Pixel/Pixel.texturepack", pack.to_string()),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    // The planks' own picture: smooth boards, wide.
    picture(&dir.path().join("Pictures/planks.png"), 64, |x, y| {
        shaded([170, 125, 70], if y % 16 == 0 { 0.6 } else { 0.9 + speck(x / 8, y / 16, 1) * 0.2 })
    });
    let pixel = dir.path().join("Packs/Pixel");
    let dirt = |x: u32, y: u32| shaded([120, 84, 56], 0.8 + speck(x, y, 2) * 0.4);
    picture(&pixel.join("grass_top.png"), 16, |x, y| match speck(x, y, 3) {
        s if s > 0.93 => [235, 215, 70],
        s => shaded([70, 160, 60], 0.75 + s * 0.5),
    });
    picture(&pixel.join("grass_side.png"), 16, |x, y| {
        let turf = 3 + (speck(x, 0, 4) * 3.0) as u32;
        if y < turf { shaded([70, 160, 60], 0.75 + speck(x, y, 3) * 0.5) } else { dirt(x, y) }
    });
    picture(&pixel.join("dirt.png"), 16, dirt);
    picture(&pixel.join("bricks.png"), 16, |x, y| {
        let row = y / 4;
        let joint = y % 4 == 3 || (x + row * 4) % 8 == 7;
        if joint { [70, 70, 76] } else { shaded([150, 150, 158], 0.85 + speck(x / 8, row, 5) * 0.3) }
    });
    picture(&pixel.join("planks.png"), 16, |x, y| {
        if y % 4 == 3 { [70, 45, 25] } else { shaded([200, 150, 60], 0.85 + speck(x / 4, y / 4, 6) * 0.3) }
    });
    dir
}

/// The project's material given its own picture, once the project knows the picture's GUID.
fn game(dir: &tempfile::TempDir) -> Game {
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    let picture = game.content.parse("Pictures/planks.png").unwrap().guid().expect("the picture is an asset");
    let material = format!("color = [1.0, 1.0, 1.0]\nroughness = 0.9\ncolor_map = \"{picture}\"\n");
    std::fs::write(dir.path().join("Materials/Planks.material"), material).unwrap();
    game.content.rescan().unwrap();
    game
}

fn field(game: &Game) -> Entity {
    let named = game.engine.world.query_ref::<(Entity, &voxear_scene::Name)>();
    named.into_iter().find(|(_, name)| name.0 == "Blocks").unwrap().0
}

/// A few blocks on the grass in front of the camera: a grass block on a dirt one, a stack of dirt, a wall of stone and a
/// wall of planks, so tops and sides of each show. The world's corner is at (-16, 0, -16).
fn build(game: &mut Game) {
    let field = field(game);
    let mut blocks = game.engine.world.get::<BlockWorld>(field).unwrap().clone();
    let grass = [([12, 6, 13], 2u16), ([12, 7, 13], 1u16)].into_iter();
    let dirt = (6..8).map(|y| ([14, y, 14], 2u16));
    let stone = (6..9).flat_map(|y| (16..19).map(move |x| ([x, y, 15], 3u16)));
    let planks = (6..8).flat_map(|y| (20..22).map(move |x| ([x, y, 13], 4u16)));
    blocks.set_blocks(grass.chain(dirt).chain(stone).chain(planks));
    game.engine.world.insert(field, blocks);
}

#[test]
fn a_world_takes_up_the_pack_worn_and_lets_go_of_it_again() {
    let dir = project();
    let mut game = game(&dir);
    game.start(None).unwrap();
    let field = field(&game);
    let drawn = |game: &Game| game.engine.world.get::<BlockWorldAsset>(field).unwrap().drawn.clone();
    let own = drawn(&game);
    let own_faces = game.engine.world.get::<BlockWorld>(field).unwrap().faces.clone();
    assert!(own.blocks.iter().take(3).all(|kind| kind.material.is_none() && kind.top.is_none()));
    assert_eq!(own.looks(), 4, "three colours and the planks' material");

    // The game's pack: grass a top, sides and a bottom of its own, dirt and stone one face each, planks their
    // own material still, whose picture the pack changes where it is drawn.
    texture_packs::wear(&mut game.content, "Packs/Pixel/Pixel.texturepack").unwrap();
    game.tick(1.0 / 60.0, None);
    let worn = drawn(&game);
    let face =
        |kind: &BlockKind| [&kind.material, &kind.top, &kind.bottom].map(|face| face.clone().unwrap_or_default());
    let grass = face(&worn.blocks[0]);
    assert!(grass.iter().all(|face| face.starts_with(texture_packs::FACE_PREFIX)), "{grass:?}");
    assert!(grass[0].ends_with(":sides") && grass[1].ends_with(":top") && grass[2].ends_with(":bottom"), "{grass:?}");
    assert!(face(&worn.blocks[1])[0].ends_with(":all") && face(&worn.blocks[2])[0].ends_with(":all"));
    assert_eq!(worn.blocks[3].material.as_deref(), Some("Materials/Planks.material"));
    assert_eq!(worn.looks(), 6, "grass's three, dirt, stone and the planks");
    // What a block is stays what it says, whatever it is drawn with.
    assert_eq!(worn.blocks[0].color, own.blocks[0].color);
    assert!(game.engine.world.get::<BlockWorld>(field).unwrap().faces != own_faces, "its faces are made again");
    // Its file is as it was: a pack is no part of a level.
    assert!(game.engine.world.get::<BlockWorldAsset>(field).unwrap().file.blocks[0].material.is_none());

    // A grain: every block of a colour alone, and the planks as they are.
    texture_packs::wear(&mut game.content, "builtin:pixel_grain").unwrap();
    game.tick(1.0 / 60.0, None);
    let grain = drawn(&game);
    for kind in grain.blocks.iter().take(3) {
        assert!(kind.material.as_deref().is_some_and(|face| face.ends_with(":grain")) && kind.top.is_none());
    }
    assert_eq!(grain.blocks[3].material.as_deref(), Some("Materials/Planks.material"));

    // Taken off: as it was made.
    texture_packs::wear(&mut game.content, "").unwrap();
    game.tick(1.0 / 60.0, None);
    assert_eq!(*drawn(&game), *own);
    assert!(game.engine.world.get::<BlockWorld>(field).unwrap().faces == own_faces);
}

#[test]
fn a_pack_is_drawn_on_the_blocks_it_names_sharp_and_goes_again_when_taken_off() {
    use voxear_render::{Renderer, RendererSettings};
    let Ok(mut renderer) = Renderer::new_offscreen(1280, 720, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    let dir = project();
    // A pack of a player's own, in a folder outside the game: grass in big squares of two colours.
    let data = tempfile::tempdir().unwrap();
    let mine = data.path().join(texture_packs::TEXTURES_FOLDER).join("Squares");
    picture(&mine.join("squares.png"), 4, |x, y| if (x + y) % 2 == 0 { [230, 40, 200] } else { [240, 240, 240] });
    let squares = "description = \"Squares.\"\npixels = true\n[[block]]\nblock = \"Blocks/Grass.block\"\n\
                   all = \"squares.png\"\n";
    std::fs::write(mine.join("Squares.texturepack"), squares).unwrap();

    let mut game = game(&dir);
    game.start(Some(&mut renderer)).unwrap();
    build(&mut game);
    let mut draw = |game: &mut Game, name: &str| {
        for _ in 0..40 {
            game.tick(1.0 / 60.0, Some(&mut renderer));
            renderer.render(&game.engine.world, 1.0).unwrap();
        }
        let (width, height, pixels) = renderer.read_pixels().unwrap();
        if let Ok(out) = std::env::var("VOXEAR_TEST_OUTPUT") {
            let file = std::fs::File::create(std::path::Path::new(&out).join(name)).unwrap();
            let mut encoder = png::Encoder::new(file, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
        }
        pixels
    };
    let unlike = |a: &[u8], b: &[u8]| {
        let pairs = a.chunks_exact(4).zip(b.chunks_exact(4));
        pairs.filter(|(a, b)| (0..3).any(|c| a[c].abs_diff(b[c]) > 8)).count() as f32 / (a.len() / 4) as f32
    };
    let plain = draw(&mut game, "texture pack none.png");

    // A grain of each block's colour: most of the picture is ground, and all of it is speckled now.
    texture_packs::wear(&mut game.content, "builtin:pixel_grain").unwrap();
    let grain = draw(&mut game, "texture pack pixel grain.png");
    assert!(unlike(&grain, &plain) > 0.08, "the grain shows: {}", unlike(&grain, &plain));

    // The game's pack: its pictures on grass, dirt, stone and, through their material, the planks.
    texture_packs::wear(&mut game.content, "Packs/Pixel/Pixel.texturepack").unwrap();
    let pixel = draw(&mut game, "texture pack pixel.png");
    assert!(unlike(&pixel, &plain) > 0.3, "the pack shows: {}", unlike(&pixel, &plain));
    assert!(unlike(&pixel, &grain) > 0.3, "and is not the grain: {}", unlike(&pixel, &grain));
    // The grass's flowers, bright yellow pixels of its top's picture, are on the ground.
    let yellow = |p: &[u8]| p[0] > 150 && p[1] > 130 && p[2] < 110 && p[0] > p[2].saturating_add(70);
    let flowers = pixel.chunks_exact(4).filter(|p| yellow(p)).count();
    assert!(flowers > 800, "{flowers} pixels of flowers");
    assert!(plain.chunks_exact(4).filter(|p| yellow(p)).count() < flowers / 20, "none without the pack");

    // The player's pack, from its folder: grass in squares of pink and white, each square one colour to its edge.
    let reference = format!("{}{}", texture_packs::FILE_PREFIX, mine.join("Squares.texturepack").display());
    texture_packs::wear(&mut game.content, &reference).unwrap();
    let squares = draw(&mut game, "texture pack squares.png");
    let pink = |p: &[u8]| p[0] > p[1].saturating_add(60) && p[2] > p[1].saturating_add(40);
    let white = |p: &[u8]| p[0].abs_diff(p[1]) < 18 && p[1].abs_diff(p[2]) < 18 && p[0] > 90;
    // Between the two: pinkish, where a smoothed picture would run one square into the next.
    let between = |p: &[u8]| !pink(p) && !white(p) && p[0] > p[1].saturating_add(15) && p[2] > p[1].saturating_add(10);
    let count = |is: &dyn Fn(&[u8]) -> bool| squares.chunks_exact(4).filter(|p| is(p)).count();
    let (pinks, whites, blurred) = (count(&pink), count(&white), count(&between));
    assert!(pinks > 100_000 && whites > 100_000, "{pinks} pink and {whites} white");
    assert!(blurred < (pinks + whites) / 12, "sharp squares: {blurred} pixels between {pinks} pink and {whites} white");

    // Taken off, everything is drawn as it was.
    texture_packs::wear(&mut game.content, "").unwrap();
    let back = draw(&mut game, "texture pack none again.png");
    assert!(unlike(&back, &plain) < 0.01, "as it was made: {}", unlike(&back, &plain));
}

/// The flat world of [`project`] with glass, a block whose material joins (the starter glass), and a pack that
/// draws stone with joining tiles: a pale tile with a dark border.
fn joined_project() -> tempfile::TempDir {
    use voxear_engine::connected_tiles;
    let dir = project();
    connected_tiles::write_glass_block(&dir.path().join("Blocks"), "Glass").unwrap();
    let mut file =
        BlockFile::parse(&std::fs::read_to_string(dir.path().join("Blocks/Field.blockworld")).unwrap()).unwrap();
    file.blocks.push(BlockKind::of("Blocks/Glass.block"));
    std::fs::write(dir.path().join("Blocks/Field.blockworld"), file.to_text()).unwrap();
    let tiles = connected_tiles::frame_tiles(16, 2, [60, 62, 70, 255], |x, y| {
        let shade = shaded([176, 178, 186], 0.9 + speck(x, y, 9) * 0.2);
        [shade[0], shade[1], shade[2], 255]
    });
    let joined = dir.path().join("Packs/Joined");
    std::fs::create_dir_all(&joined).unwrap();
    let mut encoder = png::Encoder::new(std::fs::File::create(joined.join("tiles.png")).unwrap(), 80, 16);
    encoder.set_color(png::ColorType::Rgba);
    encoder.write_header().unwrap().write_image_data(&tiles).unwrap();
    let pack = "description = \"Stone in slabs that join.\"\npixels = true\n\n\
                [[block]]\nblock = \"Blocks/Stone.block\"\nall = \"Packs/Joined/tiles.png\"\nconnected = true\n";
    std::fs::write(joined.join("Joined.texturepack"), pack).unwrap();
    dir
}

/// Glass and stone on the grass in front of the camera: a wall of glass five panes wide and three high with a
/// pane more on top, a pane alone, a floor of stone three by three, and a wall of stone with a corner missing.
fn build_joined(game: &mut Game) {
    let field = field(game);
    let mut blocks = game.engine.world.get::<BlockWorld>(field).unwrap().clone();
    let wall = (6..9).flat_map(|y| (13..18).map(move |x| ([x, y, 16], 5u16)));
    let more = [([15, 9, 16], 5u16), ([11, 6, 13], 5u16)];
    let floor = (12..15).flat_map(|z| (19..22).map(move |x| ([x, 6, z], 3u16)));
    let stone = (6..9).flat_map(|y| (19..22).map(move |x| ([x, y, 17], 3u16))).filter(|(at, _)| *at != [21, 8, 17]);
    blocks.set_blocks(wall.chain(more).chain(floor).chain(stone));
    game.engine.world.insert(field, blocks);
}

#[test]
fn glass_joins_by_its_material_and_a_pack_makes_a_blocks_faces_join() {
    use voxear_terrain::blocks::mesh;
    let dir = joined_project();
    let mut game = game(&dir);
    game.start(None).unwrap();
    build_joined(&mut game);
    game.tick(1.0 / 60.0, None);
    let field = field(&game);
    let faces = |game: &Game| game.engine.world.get::<BlockWorld>(field).unwrap().faces.clone();
    // Glass joins on every face, by its material, and is seen through; stone, a colour alone, does not.
    let own = faces(&game);
    assert_eq!((own[5].connects, own[5].see_through), (7, true));
    assert_eq!(own[3].connects, 0);
    let quads = |game: &Game, look: u8| -> Vec<[u32; 2]> {
        let blocks = game.engine.world.get::<BlockWorld>(field).unwrap();
        let all =
            blocks.meshes.values().flat_map(|chunk| chunk.quads[..chunk.buckets[mesh::FACINGS] as usize].to_vec());
        all.filter(|quad| quad[1] as u8 == look).collect()
    };
    // The wall's panes: a face each on its two sides and round its edge, none between two panes. Sixteen panes
    // in the wall and the one on top, and one alone: 2 * 16 faces front and back, the wall's edge of 5 + 5 + 3 +
    // 3 + 2 more round the pane on top, and the lone pane's six, less the faces on the ground, which are hidden.
    let glass = quads(&game, own[5].sides);
    assert_eq!(glass.len(), 32 + (4 + 3 + 3 + 3) + 5, "{} faces of glass", glass.len());
    assert!(glass.iter().any(|&quad| mesh::joins(quad) == 0xFF), "a pane in the wall's middle joins all round");
    assert!(glass.iter().filter(|&&quad| mesh::joins(quad) == 0).count() >= 5, "the lone pane joins nothing");

    // The pack: stone's faces join, each a face of its own.
    let merged = quads(&game, own[3].sides).len();
    texture_packs::wear(&mut game.content, "Packs/Joined/Joined.texturepack").unwrap();
    game.tick(1.0 / 60.0, None);
    let worn = faces(&game);
    assert_eq!(worn[3].connects, 7);
    let stone = quads(&game, worn[3].sides);
    assert!(stone.len() > merged && stone.iter().any(|&quad| mesh::joins(quad) != 0), "{merged} to {}", stone.len());
    texture_packs::wear(&mut game.content, "").unwrap();
    game.tick(1.0 / 60.0, None);
    assert_eq!(faces(&game)[3].connects, 0);
    assert_eq!(quads(&game, own[3].sides).len(), merged, "merged again");
}

#[test]
fn faces_that_join_are_drawn_as_one_sheet_with_a_border_round_the_whole() {
    use voxear_render::{Renderer, RendererSettings};
    let Ok(mut renderer) = Renderer::new_offscreen(1280, 720, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    let dir = joined_project();
    let mut game = game(&dir);
    game.start(Some(&mut renderer)).unwrap();
    build_joined(&mut game);
    let mut draw = |game: &mut Game, name: &str| {
        for _ in 0..40 {
            game.tick(1.0 / 60.0, Some(&mut renderer));
            renderer.render(&game.engine.world, 1.0).unwrap();
        }
        let (width, height, pixels) = renderer.read_pixels().unwrap();
        if let Ok(out) = std::env::var("VOXEAR_TEST_OUTPUT") {
            let file = std::fs::File::create(std::path::Path::new(&out).join(name)).unwrap();
            let mut encoder = png::Encoder::new(file, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
        }
        pixels
    };
    let unlike = |a: &[u8], b: &[u8]| {
        let pairs = a.chunks_exact(4).zip(b.chunks_exact(4));
        pairs.filter(|(a, b)| (0..3).any(|c| a[c].abs_diff(b[c]) > 8)).count() as f32 / (a.len() / 4) as f32
    };
    let own = draw(&mut game, "joined glass.png");
    texture_packs::wear(&mut game.content, "Packs/Joined/Joined.texturepack").unwrap();
    let worn = draw(&mut game, "joined glass and stone.png");
    assert!(unlike(&own, &worn) > 0.02, "the stone's tiles show: {}", unlike(&own, &worn));
}

/// The flat world of [`project`] with its blocks in an order: grass over dirt over stone. And a pack that has each
/// keep to itself again.
fn ordered_project() -> tempfile::TempDir {
    let dir = project();
    let block = |color: [f32; 3], overlap: u8| BlockDef { overlap, ..BlockDef::colored(color) }.to_text();
    std::fs::write(dir.path().join("Blocks/Grass.block"), block([0.25, 0.55, 0.15], 2)).unwrap();
    std::fs::write(dir.path().join("Blocks/Dirt.block"), block([0.42, 0.3, 0.2], 1)).unwrap();
    let apart = "description = \"Each block to itself.\"\n\n[[block]]\nblock = \"Blocks/Grass.block\"\noverlap = 0\n\n\
                 [[block]]\nblock = \"Blocks/Dirt.block\"\noverlap = 0\n";
    std::fs::create_dir_all(dir.path().join("Packs/Apart")).unwrap();
    std::fs::write(dir.path().join("Packs/Apart/Apart.texturepack"), apart).unwrap();
    dir
}

/// Stone and dirt let into the grass in front of the camera, level with it: a floor of stone five by four with a
/// block of grass in it, dirt beside the stone, and a step of stone up from the grass, whose top is another plane.
fn build_ordered(game: &mut Game) {
    let field = field(game);
    let mut blocks = game.engine.world.get::<BlockWorld>(field).unwrap().clone();
    let stone = (10..14).flat_map(|z| (12..17).map(move |x| ([x, 5, z], 3u16)));
    let dirt = (10..14).flat_map(|z| (17..20).map(move |x| ([x, 5, z], 2u16)));
    let more = [([14, 5, 12], 1u16), ([21, 6, 13], 3u16), ([22, 6, 13], 1u16)];
    blocks.set_blocks(stone.chain(dirt).chain(more));
    game.engine.world.insert(field, blocks);
}

#[test]
fn grass_creeps_over_the_edge_of_the_dirt_and_stone_beside_it_and_a_pack_can_keep_it_to_itself() {
    use voxear_render::{Renderer, RendererSettings};
    use voxear_terrain::blocks::{file::MIXED_LOOK, mesh};
    let dir = ordered_project();
    let mut game = game(&dir);
    game.start(None).unwrap();
    build_ordered(&mut game);
    game.tick(1.0 / 60.0, None);
    let field = field(&game);
    // Each block can take another's edge, and lies where its file says in their order; every look has its twin.
    let faces = game.engine.world.get::<BlockWorld>(field).unwrap().faces.clone();
    assert_eq!([faces[1].overlap, faces[2].overlap, faces[3].overlap], [2, 1, 0]);
    assert!(faces[1..4].iter().all(|faces| faces.mixes == 7));
    let drawn = game.engine.world.get::<BlockWorldAsset>(field).unwrap().drawn.clone();
    let (looks, _) = drawn.faces();
    assert_eq!((drawn.looks(), looks.len()), (4, usize::from(MIXED_LOOK) + 4), "its looks, and a twin of each");
    assert!(looks[usize::from(MIXED_LOOK)].mixed && looks[usize::from(MIXED_LOOK)].color == looks[0].color);
    // What creeps over each top that takes an edge: grass over stone and dirt, dirt over stone where no grass is.
    let over = |game: &Game| -> Vec<(u8, u8)> {
        let blocks = game.engine.world.get::<BlockWorld>(field).unwrap();
        let quads =
            blocks.meshes.values().flat_map(|chunk| chunk.quads[..chunk.buckets[mesh::FACINGS] as usize].to_vec());
        let tops = quads.filter(|quad| (quad[0] >> 25) & 7 == 2 && quad[1] as u8 & MIXED_LOOK != 0);
        tops.map(|quad| (quad[1] as u8 & !MIXED_LOOK, mesh::mixing(quad).look)).collect()
    };
    let mixed = over(&game);
    let count = |under: u8, creeping: u8| mixed.iter().filter(|pair| **pair == (under, creeping)).count();
    let (grass, dirt, stone) = (faces[1].top, faces[2].top, faces[3].top);
    assert!(count(stone, grass) >= 12 && count(dirt, grass) >= 8 && count(stone, dirt) >= 2, "{mixed:?}");
    assert_eq!(count(grass, stone) + count(grass, dirt) + count(dirt, stone), 0, "never the lower over the higher");
    // A pack that has grass and dirt keep to themselves: no face takes an edge.
    texture_packs::wear(&mut game.content, "Packs/Apart/Apart.texturepack").unwrap();
    game.tick(1.0 / 60.0, None);
    assert!(over(&game).is_empty());
    texture_packs::wear(&mut game.content, "").unwrap();
    game.tick(1.0 / 60.0, None);
    assert_eq!(over(&game).len(), mixed.len());

    let Ok(mut renderer) = Renderer::new_offscreen(1280, 720, &RendererSettings::default()) else {
        eprintln!("skipping the pictures: no usable GPU");
        return;
    };
    let mut game = self::game(&dir);
    game.start(Some(&mut renderer)).unwrap();
    build_ordered(&mut game);
    let mut draw = |game: &mut Game, name: &str| {
        for _ in 0..40 {
            game.tick(1.0 / 60.0, Some(&mut renderer));
            renderer.render(&game.engine.world, 1.0).unwrap();
        }
        let (width, height, pixels) = renderer.read_pixels().unwrap();
        if let Ok(out) = std::env::var("VOXEAR_TEST_OUTPUT") {
            let file = std::fs::File::create(std::path::Path::new(&out).join(name)).unwrap();
            let mut encoder = png::Encoder::new(file, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
        }
        pixels
    };
    let unlike = |a: &[u8], b: &[u8]| {
        let pairs = a.chunks_exact(4).zip(b.chunks_exact(4));
        pairs.filter(|(a, b)| (0..3).any(|c| a[c].abs_diff(b[c]) > 24)).count()
    };
    // In their own colours, then kept apart, then under the game's pack of pixel pictures.
    let creeping = draw(&mut game, "mixed colours.png");
    texture_packs::wear(&mut game.content, "Packs/Apart/Apart.texturepack").unwrap();
    let apart = draw(&mut game, "mixed apart.png");
    let fringe = unlike(&creeping, &apart);
    assert!((1500..60_000).contains(&fringe), "a fringe along the edges, and no more: {fringe} pixels");
    texture_packs::wear(&mut game.content, "Packs/Pixel/Pixel.texturepack").unwrap();
    draw(&mut game, "mixed pixel.png");
}
