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
    let (ground, biomes, size) = voxear_engine::blocks::world_ground(&mut content, guid).expect("a generated world");
    eprintln!("blocks {size} m, {} biomes", biomes.as_ref().map_or(0, |biomes| biomes.biomes.list().len()));
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
    match ground.gpu_program() {
        Ok(program) => eprintln!(
            "runs on the GPU: {} steps, {} slots, {} MB of maps, volume {}, leaving out {:?}",
            program.steps,
            program.slots,
            program.data.len() * 4 / 1_000_000,
            program.volume.is_some(),
            program.left_out
        ),
        Err(names) => eprintln!("not on the GPU for {names:?}"),
    }
}
