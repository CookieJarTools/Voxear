//! What of a world of blocks' ground generator runs on the GPU, for an
//! endless Voxel Volume made from it: run by hand on a project that has
//! one, as `VOXEAR_WORLD=/path/to/project/Blocks/Realm.blockworld cargo test
//! -p voxear_engine --test realm_on_gpu -- --ignored --nocapture`.

use voxear_engine::Content;

#[test]
#[ignore = "needs a project with a world of blocks, named by VOXEAR_WORLD"]
fn a_world_of_blocks_ground_generator_says_what_runs_on_the_gpu() {
    let Ok(file) = std::env::var("VOXEAR_WORLD") else { return };
    let file = std::path::PathBuf::from(file);
    let root = file.ancestors().find(|dir| dir.join("voxear.toml").is_file()).expect("a project round it");
    let mut content = Content::open(root).unwrap();
    let path = file.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
    let guid = content.db().guid_of(&path).expect("the world of blocks in its project");
    let found = voxear_engine::blocks::world_ground(&mut content, guid).expect("a generated world");
    let found_colours = found.colours.clone();
    let (ground, biomes, size) = (found.generator, found.biomes, found.block_size);
    eprintln!("blocks {size} m, {} biomes", biomes.as_ref().map_or(0, |biomes| biomes.biomes.list().len()));
    if let Some(biomes) = &biomes {
        let painted = biomes.painted.as_ref().is_some_and(|painted| !painted.maps().biome.is_empty());
        let strata = biomes.blocks.iter().filter(|blocks| blocks.strata.is_some()).count();
        eprintln!(
            "regions {}, painted biomes {painted}, sunk {}, steep {}, {strata} with rock layers",
            biomes.regions.is_some(),
            biomes.any_sunk(),
            biomes.any_steep()
        );
    }
    // Its ground round a place, and over the whole world, as the CPU makes it: where to look.
    if let Ok(around) = std::env::var("VOXEAR_AROUND") {
        let at: Vec<f32> = around.split(',').map(|part| part.trim().parse().unwrap()).collect();
        for dz in [-60.0, -30.0, 0.0, 30.0, 60.0] {
            let row: Vec<String> = [-60.0, -30.0, 0.0, 30.0, 60.0]
                .iter()
                .map(|dx| format!("{:7.1}", ground.height_at(at[0] + dx, at[1] + dz)))
                .collect();
            eprintln!("{}", row.join(" "));
        }
    }
    let mut steepest = (0.0f32, [0.0f32; 2]);
    for z in (0..51200).step_by(800) {
        for x in (0..51200).step_by(800) {
            let (x, z) = (x as f32, z as f32);
            let h = ground.height_at(x, z);
            let rise = (ground.height_at(x + 40.0, z) - h).abs() + (ground.height_at(x, z + 40.0) - h).abs();
            if h > 2.0 && rise > steepest.0 {
                steepest = (rise, [x, z]);
            }
        }
    }
    eprintln!("steepest place over the sea: {:?}, {} m up", steepest, ground.height_at(steepest.1[0], steepest.1[1]));
    // The biome the CPU picks at a few places round there, and its blocks' colours.
    if let (Some(biomes), Ok(around)) = (&biomes, std::env::var("VOXEAR_AROUND")) {
        let at: Vec<f32> = around.split(',').map(|part| part.trim().parse().unwrap()).collect();
        for d in [-150.0, -60.0, 0.0, 60.0, 150.0] {
            let (x, z) = (at[0] + d, at[1] + d * 0.5);
            let height = ground.height_at(x, z);
            let (temperature, humidity) = ground.climate(x, z, height);
            let above_sea = height - ground.sea_level();
            let place = voxear_terrain::generate::biomes::Place { temperature, humidity, above_sea, sunk: 0.0 };
            let index = biomes.index_at(x, z, place);
            let blocks = index.and_then(|index| biomes.blocks.get(index));
            let colour = |block: u16| found_colours.get(usize::from(block)).copied();
            eprintln!(
                "({x}, {z}) {height:.1} m {temperature:.1} °C {humidity:.2}: biome {index:?}, top {:?} steep {:?}",
                blocks.map(|b| colour(b.top)),
                blocks.map(|b| colour(b.steep))
            );
        }
    }
    // Where its Volume stands over the ground (spires, bridges), and where its rivers' water is, round a place.
    if let (Ok(around), Some(volume)) = (std::env::var("VOXEAR_FIND"), ground.volume()) {
        let at: Vec<f32> = around.split(',').map(|part| part.trim().parse().unwrap()).collect();
        let (side, step) = (400usize, 16.0f32);
        let (xs, zs): (Vec<f32>, Vec<f32>) = (0..side * side)
            .map(|i| {
                (
                    at[0] + ((i % side) as f32 - side as f32 / 2.0) * step,
                    at[1] + ((i / side) as f32 - side as f32 / 2.0) * step,
                )
            })
            .unzip();
        let mut heights = vec![0.0; xs.len()];
        ground.heights(&xs, &zs, &mut heights);
        let standing = volume.standing(&xs, &zs, &heights);
        let mut shown = 0;
        for i in (0..xs.len()).filter(|&i| !standing[i].is_empty()) {
            if shown < 40 {
                eprintln!("stands at ({}, {}) ground {:.1}: {:?}", xs[i], zs[i], heights[i], standing[i]);
            }
            shown += 1;
        }
        eprintln!("{shown} of {} columns have something standing", xs.len());
        if let Some((water, _)) = ground.water(&xs, &zs) {
            let wet: Vec<usize> =
                (0..xs.len()).filter(|&i| water[i] > -1.0e5 && water[i] > ground.sea_level() + 1.0).collect();
            eprintln!("{} columns have river or lake water over the sea", wet.len());
            for &i in wet.iter().step_by((wet.len() / 30).max(1)) {
                eprintln!("water at ({}, {}) {:.1} over ground {:.1}", xs[i], zs[i], water[i], heights[i]);
            }
        }
    }
    // The water its chunks are made with, by the generator they are made by, against the ground's.
    if let Ok(around) = std::env::var("VOXEAR_WATER") {
        let at: Vec<f32> = around.split(',').map(|part| part.trim().parse().unwrap()).collect();
        let loaded = content.block_world(guid).unwrap();
        let asset = voxear_engine::blocks::BlockWorldAsset {
            guid,
            file: loaded.file.clone(),
            base: loaded.base.clone(),
            pending: loaded.pending.clone(),
            from_file: loaded.file.blocks.len(),
            drawn: std::sync::Arc::new(voxear_engine::blocks::resolved(&mut content, &loaded.file)),
            derived: loaded.derived,
        };
        let making = voxear_engine::blocks::making(&mut content, &asset).unwrap();
        let chunks = voxear_terrain::blocks::generate::ground_generator(&making.generate, &making.world, size);
        eprintln!("chunks' generator {:#x}, the volume's {:#x}", chunks.hash(), ground.hash());
        for dz in (-80..=80).step_by(16) {
            let (xs, zs): (Vec<f32>, Vec<f32>) =
                (-80..=80).step_by(16).map(|dx| (at[0] + dx as f32, at[1] + dz as f32)).unzip();
            let show = |world: &voxear_terrain::generate::WorldGen| -> String {
                let water = world.water(&xs, &zs).map(|(water, _)| water).unwrap_or_default();
                water.iter().map(|w| if *w > -1.0e5 { format!("{w:5.1}") } else { "    .".into() }).collect()
            };
            eprintln!("{dz:4}: {} | {}", show(&chunks), show(&ground));
        }
    }
    // How long what the GPU's spires and bridges read takes to find on the CPU, over a square round a place.
    if let (Ok(around), Ok(program)) = (std::env::var("VOXEAR_FINDS"), ground.gpu_program()) {
        let at: Vec<f32> = around.split(',').map(|part| part.trim().parse().unwrap()).collect();
        for finder in &program.finds {
            let finds = &finder.0;
            let side = (at[2] / finds.cell()).ceil() as i32;
            let first = [(at[0] / finds.cell()) as i32 - side / 2, (at[1] / finds.cell()) as i32 - side / 2];
            let started = std::time::Instant::now();
            let mut found = 0;
            for z in 0..side {
                for x in 0..side {
                    found += usize::from(finds.record([first[0] + x, first[1] + z])[0] != 0);
                }
            }
            let took = started.elapsed();
            eprintln!(
                "{:?}: {} cells in {:.2} s, {:.3} ms each, {found} with something",
                finder,
                side * side,
                took.as_secs_f32(),
                took.as_secs_f32() * 1000.0 / (side * side) as f32
            );
        }
    }
    // An endless volume made by it finds its rivers' water where the generator has it.
    if let Ok(around) = std::env::var("VOXEAR_WATER_AT") {
        use voxear_core::math::Vec3;
        let at: Vec<f32> = around.split(',').map(|part| part.trim().parse().unwrap()).collect();
        let volume = voxear_render::VoxelVolume {
            size: Vec3::new(16.0, 720.0, 16.0),
            endless: true,
            generator: Some(voxear_render::VoxelGenerator {
                world: ground.clone(),
                floor: -60.0,
                at: [0.0, 0.0],
                biomes: None,
            }),
            ..voxear_render::VoxelVolume::default()
        };
        let corner = Vec3::new(0.0, -60.0, 0.0);
        let (water, _) = ground.water(&[at[0]], &[at[1]]).unwrap();
        let found = voxear_render::voxel_water_at(&volume, corner, Vec3::new(at[0], water[0] - 0.3, at[1]));
        eprintln!("the generator's water at {at:?}: {:.2}; the volume's: {found:?}", water[0]);
    }
    eprintln!("has rivers: {}", ground.has_rivers());
    match ground.gpu_program() {
        Ok(program) => eprintln!(
            "runs on the GPU: {} steps, {} slots, {} MB of maps, volume {:?}, {} found on the CPU, leaving out {:?}",
            program.steps,
            program.slots,
            program.data.len() * 4 / 1_000_000,
            program.volume.as_ref().map(|v| (v.steps, v.slots, v.carried.len(), v.spans.len())),
            program.finds.len(),
            program.left_out
        ),
        Err(names) => eprintln!("not on the GPU for {names:?}"),
    }
}

