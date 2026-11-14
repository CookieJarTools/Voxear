//! A script reads the post-processing in effect and lays its own over the
//! level's through `Post`, and asks for HDR output through `Screen`, for
//! whatever shows the game to make so. Builds a copy of Hello's scripts with
//! one more; skips itself without .NET and the managed build.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, post_scripts, scripts};
use voxear_render::{
    AmbientOcclusionQuality, PostProcess, Tonemapper,
    display::{DisplaySettings, HdrChoice},
};

const SCRIPT: &str = r#"using Voxear;

namespace Hello;

public sealed class Looks : SystemBase
{
	protected override void OnCreate()
	{
		Log.Info($"defaults {new PostSettings().ToJson()}");
		Log.Info($"bloom before {Post.Current.Bloom.Intensity}");
		Post.Change(post =>
		{
			post.Bloom.Intensity = 0.3f;
			post.AntiAliasing.RenderScale = 0.5f;
			post.AmbientOcclusion.Quality = AmbientOcclusionQuality.Ultra;
			post.Grading.Lift = new System.Numerics.Vector3(0.1f, 0f, 0f);
		});
		Log.Info($"changes {Post.ChangesToJson()}");
		Log.Info($"bloom after {Post.Current.Bloom.Intensity}");
		Post.Change(post => post.AmbientOcclusion.Quality = (AmbientOcclusionQuality)9);
		Log.Info($"quality refused {Post.Current.AmbientOcclusion.Quality}");
		Screen.SetHdr(HdrMode.On, 250f, 900f);
		Log.Info($"hdr {Screen.Hdr} showing {Screen.HdrOutput} offered {Screen.HdrAvailable} paper {Screen.PaperWhite}");
	}

	protected override void Update()
	{
	}
}
"#;

/// A copy of Hello, inside the repository, where its scripts find the
/// engine to build against.
fn hello_copy() -> tempfile::TempDir {
    fn copy(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "bin" || name == "obj" {
                continue;
            }
            if entry.path().is_dir() {
                copy(&entry.path(), &to.join(&name));
            } else {
                std::fs::copy(entry.path(), to.join(&name)).unwrap();
            }
        }
    }
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    copy(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/Hello"), dir.path());
    dir
}

#[test]
fn scripts_change_the_post_processing_and_ask_for_hdr() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Looks.cs"), SCRIPT).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    // The level's own settings, as a scene's [post] would give them.
    let level = PostProcess {
        tonemap: voxear_render::Tonemap { mapper: Tonemapper::Aces, ..Default::default() },
        ..PostProcess::voxear()
    };
    game.engine.world.insert_resource(level);
    game.tick(1.0 / 60.0, None);

    let messages: Vec<String> = log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect();
    let said = |start: &str| {
        messages
            .iter()
            .find_map(|m| m.strip_prefix(start))
            .unwrap_or_else(|| panic!("no {start:?}: {messages:?}"))
            .to_string()
    };

    // C#'s settings start as the engine's Voxear Look, setting by setting.
    let defaults: serde_json::Value = serde_json::from_str(&said("defaults ")).unwrap();
    let ours = serde_json::to_value(PostProcess::voxear()).unwrap();
    for (section, settings) in ours.as_object().unwrap() {
        let theirs = &defaults[section];
        match settings.as_object() {
            Some(settings) => {
                for (key, value) in settings {
                    let (a, b) = (value, &theirs[key]);
                    let near = match (a.as_f64(), b.as_f64()) {
                        (Some(a), Some(b)) => (a - b).abs() < 1e-4,
                        _ => {
                            a == b
                                || (a.is_array()
                                    && a.as_array()
                                        .unwrap()
                                        .iter()
                                        .zip(b.as_array().unwrap())
                                        .all(|(x, y)| (x.as_f64().unwrap() - y.as_f64().unwrap()).abs() < 1e-4))
                        }
                    };
                    assert!(near, "{section}.{key}: the engine's {a}, C#'s {b}");
                }
            }
            None => assert!((settings.as_f64().unwrap() - theirs.as_f64().unwrap()).abs() < 1e-4, "{section}"),
        }
    }

    // What the script changed is laid over the level's, and nothing else.
    let post = post_scripts::in_effect(&game.engine.world);
    assert_eq!(post.bloom.intensity, 0.3);
    assert_eq!(post.anti_aliasing.render_scale, 0.5);
    assert_eq!(post.ambient_occlusion.quality, AmbientOcclusionQuality::Ultra);
    assert!((post.grading.lift[0] - 0.1).abs() < 1e-6);
    assert_eq!(post.tonemap.mapper, Tonemapper::Aces, "the level's own, which the script left alone");
    assert_eq!(said("bloom after "), "0.3");
    let changes: serde_json::Value = serde_json::from_str(&said("changes ")).unwrap();
    assert_eq!(changes["bloom"], serde_json::json!({ "intensity": 0.3 }));
    assert!(changes.get("tonemap").is_none() && changes.get("fog").is_none(), "{changes}");
    // A quality there is none of is refused, and what was set stays.
    assert_eq!(said("quality refused "), "Ultra");
    assert_eq!(post_scripts::in_effect(&game.engine.world).bloom.intensity, 0.3);

    // HDR asked for, for the window to make so; with none here, SDR is shown.
    assert_eq!(game.display.hdr, DisplaySettings { hdr: HdrChoice::On, paper_white: 250.0, peak: 900.0 });
    assert!(game.display.hdr_changed);
    assert_eq!(said("hdr "), "On showing Sdr offered False paper 250");
}
