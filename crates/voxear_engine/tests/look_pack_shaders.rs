//! A look pack that names a look shader of the game's: worn, the frame is
//! drawn with the shader's number and its properties' values, the pack's
//! over the shader's own; and a pack naming a shader that draws materials,
//! or none there is, is not worn. No GPU: what the renderer is handed.

use voxear_engine::{Engine, Game, Project, look_packs};
use voxear_render::WornLook;

const PRINT: &str = "struct Print\n{\n    [Property, Range(2, 16)] float steps = 6.0;\n    \
                     [Property, Color] float3 paper = float3(1.0, 1.0, 1.0);\n}\n\
                     float3 look(Print print, LookInput input)\n{\n    \
                     return floor(input.color * print.steps) / print.steps * print.paper;\n}\n";

const PAINT: &str = "struct Paint\n{\n    [Property, Color] float3 tint = float3(1.0, 0.0, 0.0);\n}\n\
                     void surface(Paint paint, SurfaceInput input, inout SurfaceOutput surface)\n{\n    \
                     surface.color = paint.tint;\n}\n";

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let files = [
        ("voxear.toml", "[project]\nname = \"Looks\"\nstart_level = \"Levels/Main.level\"\n"),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Empty.scene\"\n"),
        ("Scenes/Empty.scene", "format = 1\n"),
        ("Shaders/Print.shader", PRINT),
        ("Shaders/Paint.shader", PAINT),
        ("Looks/Printed.lookpack", "[shader]\nshader = \"Shaders/Print.shader\"\nsteps = 4\n"),
        ("Looks/Painted.lookpack", "[shader]\nshader = \"Shaders/Paint.shader\"\n"),
        ("Looks/Lost.lookpack", "[shader]\nshader = \"Shaders/Gone.shader\"\n"),
        ("Looks/Stray.lookpack", "[shader]\nshader = \"Shaders/Print.shader\"\nink = 1.0\n"),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

#[test]
fn a_pack_wears_a_look_shader_of_the_games_and_refuses_one_that_is_not() {
    let dir = project();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    game.tick(1.0 / 60.0, None);
    assert!(game.engine.world.resource::<WornLook>().is_none());

    look_packs::wear(&mut game.engine.world, &mut game.content, "Looks/Printed.lookpack").unwrap();
    game.tick(1.0 / 60.0, None);
    let shader = game.engine.world.resource::<WornLook>().and_then(|worn| worn.shader).expect("the pack's shader");
    let registry = game.content.shaders();
    let print = registry.shaders.iter().find(|shader| shader.path == "Shaders/Print.shader").expect("read");
    assert_eq!(shader.number, print.number, "by its number among the project's shaders");
    assert_eq!((shader.values[0][0], shader.values[1]), (4.0, [1.0; 4]), "the pack's steps, the shader's own paper");

    // A shader that draws materials, one there is not, and a property the shader has not: none is worn, each
    // says why, and the pack worn stays.
    for (pack, why) in [("Painted", "no look function"), ("Lost", "Gone.shader"), ("Stray", "no property named ink")] {
        let refused = look_packs::wear(&mut game.engine.world, &mut game.content, &format!("Looks/{pack}.lookpack"));
        let said = format!("{:#}", refused.expect_err(pack));
        assert!(said.contains(why), "{pack}: {said}");
    }
    game.tick(1.0 / 60.0, None);
    assert_eq!(game.engine.world.resource::<WornLook>().and_then(|worn| worn.shader), Some(shader));

    // Taken off, the frame is drawn with none.
    look_packs::wear(&mut game.engine.world, &mut game.content, "").unwrap();
    game.tick(1.0 / 60.0, None);
    assert!(game.engine.world.resource::<WornLook>().is_none());
}
