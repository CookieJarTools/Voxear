//! Shader packs as a game wears them: from the engine, the project and a
//! folder of the player's; worn at once or compiled on a thread; drawn again
//! as their files are saved, a mistake leaving the pack drawn before; and
//! their options kept as they are compiled again. No GPU: what the renderer
//! is handed.

use std::time::{Duration, Instant};

use voxear_engine::{
    Engine, Game, Project,
    look_packs::PackSource,
    shader_packs::{self, Standing},
};
use voxear_render::shader_packs::WornShaderPack;

const GRADE: &str =
    "float4 run(PassInput input)\n{\n    return float4(sceneColor(input.uv).rgb * options().strength, 1.0);\n}\n";

const MANIFEST: &str = r#"
description = "A test grade."

[options]
strength = { kind = "number", range = [0.0, 2.0], default = 1.0, label = "Strength" }
look = { kind = "choice", choices = ["Soft", "Hard"], default = "Soft" }

[[pass]]
name = "Grade"
program = "grade.slang"
at = "final"
reads = ["color"]
"#;

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let files = [
        ("voxear.toml", "[project]\nname = \"Packs\"\nstart_level = \"Levels/Main.level\"\n"),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Empty.scene\"\n"),
        ("Scenes/Empty.scene", "format = 1\n"),
        ("Packs/Warm.shaderpack/pack.toml", MANIFEST),
        ("Packs/Warm.shaderpack/grade.slang", GRADE),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

fn game(dir: &tempfile::TempDir) -> Game {
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    game.tick(1.0 / 60.0, None);
    game
}

fn drawn(game: &Game) -> Option<u64> {
    game.engine.world.resource::<WornShaderPack>().map(|worn| worn.pack.generation)
}

/// Ticks until `done` or ten seconds have passed.
fn tick_until(game: &mut Game, done: impl Fn(&Game) -> bool) -> bool {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(10) {
        game.tick(1.0 / 60.0, None);
        if done(game) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn the_engines_the_projects_and_a_players_packs_are_listed_and_worn() {
    let dir = project();
    let mut game = game(&dir);
    let players = tempfile::tempdir().unwrap();
    let own = players.path().join("Mine.shaderpack");
    std::fs::create_dir_all(&own).unwrap();
    std::fs::write(own.join("pack.toml"), MANIFEST).unwrap();
    std::fs::write(own.join("grade.slang"), GRADE).unwrap();
    let packs = shader_packs::list(&mut game.content, Some(players.path()));
    let names: Vec<(&str, PackSource)> = packs.iter().map(|pack| (pack.name.as_str(), pack.source)).collect();
    assert_eq!(names, [("Evening", PackSource::Builtin), ("Warm", PackSource::Game), ("Mine", PackSource::Player)]);
    assert_eq!(packs[1].description, "A test grade.");

    // The project's, by its manifest's path: known by its GUID from then on, and drawn.
    let world = &mut game.engine.world;
    shader_packs::wear(world, &mut game.content, "Packs/Warm.shaderpack/pack.toml").unwrap();
    assert_eq!(shader_packs::worn(&game.engine.world), Some(packs[1].reference.as_str()));
    assert_eq!(shader_packs::standing(&game.engine.world), Some(Standing::Drawn));
    let worn = game.engine.world.resource::<WornShaderPack>().expect("handed to the renderer");
    assert_eq!((worn.pack.passes.len(), worn.options[..2].to_vec()), (1, vec![1.0, 0.0]));

    // The player's, by its folder; the engine's, by its name; and none.
    for reference in [packs[2].reference.as_str(), "builtin:evening"] {
        shader_packs::wear(&mut game.engine.world, &mut game.content, reference).unwrap();
        assert_eq!(shader_packs::worn(&game.engine.world), Some(reference));
        assert!(drawn(&game).is_some(), "{reference} drawn");
    }
    let evening = game.engine.world.resource::<WornShaderPack>().unwrap().pack.clone();
    assert_eq!(evening.passes.len(), 3, "rays, the light they give, and the grade");
    assert_eq!(evening.modules.len(), 1, "and the program that lights its block faces");
    shader_packs::wear(&mut game.engine.world, &mut game.content, "").unwrap();
    game.tick(1.0 / 60.0, None);
    assert!(drawn(&game).is_none() && shader_packs::worn(&game.engine.world).is_none());
    // One there is not leaves none worn, and says so.
    let refused = shader_packs::wear(&mut game.engine.world, &mut game.content, "builtin:gloomy");
    assert!(format!("{:#}", refused.unwrap_err()).contains("no built-in shader pack named `gloomy`"));
}

#[test]
fn a_pack_worn_soon_compiles_on_a_thread_and_is_drawn_again_as_its_files_are_saved() {
    let dir = project();
    let mut game = game(&dir);
    let world = &mut game.engine.world;
    shader_packs::wear_soon(world, &mut game.content, "Packs/Warm.shaderpack/pack.toml", true).unwrap();
    assert_eq!(shader_packs::standing(&game.engine.world), Some(Standing::Compiling));
    assert!(tick_until(&mut game, |game| drawn(game).is_some()), "compiled and put on");
    let first = drawn(&game).unwrap();
    assert_eq!(shader_packs::standing(&game.engine.world), Some(Standing::Drawn));
    assert_eq!(shader_packs::set_option(&mut game.engine.world, "strength", 1.6).unwrap(), 1.6);

    // Saved again, with an option more: compiled again, the option set kept and the new one at its default.
    let program = dir.path().join("Packs/Warm.shaderpack/grade.slang");
    std::fs::write(&program, GRADE.replace("* options().strength", "* options().strength * 0.5")).unwrap();
    let manifest = dir.path().join("Packs/Warm.shaderpack/pack.toml");
    let more = MANIFEST.replace("[[pass]]", "glow = { kind = \"toggle\", default = true }\n\n[[pass]]");
    std::fs::write(&manifest, more).unwrap();
    assert!(tick_until(&mut game, |game| drawn(game).is_some_and(|now| now != first)), "drawn again");
    let options: Vec<(String, f32)> =
        shader_packs::options(&game.engine.world).into_iter().map(|(option, value)| (option.key, value)).collect();
    assert_eq!(options, [("strength".into(), 1.6), ("look".into(), 0.0), ("glow".into(), 1.0)]);
    assert_eq!(game.engine.world.resource::<WornShaderPack>().unwrap().options[0], 1.6);

    // A mistake is said with its file and line, and the pack drawn before carries on.
    let second = drawn(&game).unwrap();
    std::fs::write(&program, "float4 run(PassInput input)\n{\n    return nothing;\n}\n").unwrap();
    let failed = |game: &Game| matches!(shader_packs::standing(&game.engine.world), Some(Standing::Failed(_)));
    assert!(tick_until(&mut game, failed), "the compile fails");
    let Some(Standing::Failed(error)) = shader_packs::standing(&game.engine.world) else { unreachable!() };
    assert!(error.contains("grade.slang:3"), "{error}");
    assert_eq!(drawn(&game), Some(second));
    // Put right, it is drawn again.
    std::fs::write(&program, GRADE).unwrap();
    assert!(tick_until(&mut game, |game| drawn(game).is_some_and(|now| now != second)), "drawn once put right");
    assert_eq!(shader_packs::standing(&game.engine.world), Some(Standing::Drawn));
}

#[test]
fn options_are_set_by_name_kept_to_what_they_can_be_and_given_to_the_frame() {
    let dir = project();
    let mut game = game(&dir);
    let world = &mut game.engine.world;
    shader_packs::wear(world, &mut game.content, "Packs/Warm.shaderpack/pack.toml").unwrap();
    assert_eq!(shader_packs::set_option(world, "strength", 9.0).unwrap(), 2.0, "kept to its range");
    let look = shader_packs::options(world).into_iter().find(|(option, _)| option.key == "look").unwrap().0;
    let hard = shader_packs::choice_value(&look, "Hard").unwrap();
    assert_eq!(shader_packs::set_option(world, "look", hard).unwrap(), 1.0);
    assert!(format!("{:#}", shader_packs::set_option(world, "size", 1.0).unwrap_err()).contains("strength, look"));
    game.tick(1.0 / 60.0, None);
    let worn = game.engine.world.resource::<WornShaderPack>().unwrap();
    assert_eq!(worn.options[..2], [2.0, 1.0]);
    let json = shader_packs::worn_json(&game.engine.world);
    assert_eq!(json["options"][1]["value"], "Hard");
    assert_eq!(json["passes"][0]["at"], "final");
    // Worn again, as when its file changed, the options set are kept.
    shader_packs::wear(&mut game.engine.world, &mut game.content, "Packs/Warm.shaderpack/pack.toml").unwrap();
    assert_eq!(game.engine.world.resource::<WornShaderPack>().unwrap().options[..2], [2.0, 1.0]);
}

#[test]
fn every_built_in_pack_compiles() {
    for (key, ..) in voxear_render::shader_packs::BUILTIN {
        let manifest = voxear_render::shader_packs::PackManifest::builtin(key).unwrap();
        let files =
            voxear_render::shader_packs::PackFiles::Embedded(voxear_render::shader_packs::builtin_files(key).unwrap());
        let compiled = voxear_render::shader_packs::compile(&manifest, &files);
        let compiled = compiled.unwrap_or_else(|error| panic!("{key}: {error:#}"));
        assert_eq!(compiled.passes.len(), manifest.passes.len());
        assert!(compiled.passes.iter().all(|spirv| spirv.len() > 1000 && spirv[..4] == [0x03, 0x02, 0x23, 0x07]));
    }
}
