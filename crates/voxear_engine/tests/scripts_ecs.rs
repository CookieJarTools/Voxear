//! The ECS from C#: systems written in C# run in a headless game, on a copy
//! of Hello with a script of their own, and the test reads what they did
//! straight from the world, the same chunk memory they wrote through spans.
//! A file of its own, so it runs in a process of its own, since .NET starts
//! only once per process. Skips itself without .NET.

use voxear_core::logging::LogCapture;
use voxear_ecs::{ComponentId, Entity, World};
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_scene::{Name, Transform};

/// Components and systems that use every part of the API: components made
/// and added in OnCreate, on new entities and on an Obj; a chunk walk that
/// writes one component from another; ForEach on an Obj's Transform;
/// commands made mid-walk; Changed filters; and an entity's Get and Set.
const SYSTEMS: &str = r#"using System.Numerics;
using Voxear;

namespace EcsCheck
{
	public struct Spot : IComponent { public Vector3 Position; }
	public struct Drift : IComponent { public Vector3 Velocity; }
	public struct Spin : IComponent { public float Speed; }
	public struct Gone : IComponent { }
	public struct Tally : IComponent { public int Frames; public int ChangedSpots; public int ChangedDrifts; }

	[UpdateIn(Stage.First)]
	public sealed class Setup : SystemBase
	{
		protected override void OnCreate()
		{
			for (var i = 0; i < 1000; i++)
			{
				Entity.Create().Add(new Spot { Position = new Vector3(i, 0f, 0f) }).Add(new Drift { Velocity = Vector3.UnitY });
			}
			Entity.Create().Add<Tally>();
			Obj.Find("Spinner A").Add(new Spin { Speed = 1f });
		}

		protected override void Update()
		{
		}
	}

	public sealed class DriftSystem : SystemBase
	{
		protected override void Update()
		{
			foreach (var chunk in Query<Spot, Drift>())
			{
				var spots = chunk.Write<Spot>();
				var drifts = chunk.Read<Drift>();
				for (var i = 0; i < chunk.Count; i++)
				{
					spots[i].Position += drifts[i].Velocity;
				}
			}
		}
	}

	// After DriftSystem, by name: destroys every entity at an even X and tags the rest, once each.
	[ExecutionOrder(10)]
	public sealed class CullSystem : SystemBase
	{
		protected override void Update()
		{
			foreach (var chunk in Query<Spot>().Without<Gone>())
			{
				var spots = chunk.Read<Spot>();
				var entities = chunk.Entities;
				for (var i = 0; i < chunk.Count; i++)
				{
					if ((int)spots[i].Position.X % 2 == 0)
					{
						entities[i].Destroy();
					}
					else
					{
						entities[i].Add<Gone>();
					}
				}
			}
		}
	}

	[UpdateIn(Stage.PostUpdate)]
	public sealed class SpinSystem : SystemBase
	{
		protected override void Update()
		{
			Query<Transform, Spin>().ForEach((ref Transform transform, ref Spin spin) => transform.Scale = new Vector3(2f));
		}
	}

	[UpdateIn(Stage.Last)]
	public sealed class TallySystem : SystemBase
	{
		protected override void Update()
		{
			var spots = Query<Spot>().Changed<Spot>().Count();
			var drifts = Query<Drift>().Changed<Drift>().Count();
			var frames = 0;
			foreach (var chunk in Query<Tally>())
			{
				var tally = chunk.Write<Tally>();
				tally[0].Frames++;
				tally[0].ChangedSpots = spots;
				tally[0].ChangedDrifts = drifts;
				frames = tally[0].Frames;
			}

			var spinner = Obj.Find("Spinner A");
			var spin = spinner.Get<Spin>();
			spin.Speed = frames;
			spinner.Set(spin);
		}
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

fn component(world: &World, name: &str) -> ComponentId {
    world.components().id_by_name(name).unwrap_or_else(|| panic!("{name} was never registered"))
}

/// Every entity with the component.
fn with(world: &World, id: ComponentId) -> Vec<Entity> {
    let archetypes = world.archetypes().iter().filter(|archetype| archetype.has(id));
    archetypes.flat_map(|a| (0..a.chunk_count()).flat_map(move |c| a.chunk_entities(c).to_vec())).collect()
}

/// A C# component's fields, read as the floats or ints they are.
fn read<T: Copy, const N: usize>(world: &World, entity: Entity, id: ComponentId) -> [T; N] {
    let value = world.get_by_id(entity, id).expect("the entity has it");
    unsafe { value.cast::<[T; N]>().read_unaligned() }
}

#[test]
fn csharp_systems_work_on_the_engines_own_chunks() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/EcsCheck.cs"), SYSTEMS).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    const FRAMES: i32 = 10;
    for _ in 0..FRAMES {
        game.tick(1.0 / 60.0, None);
    }
    // tracing ranks the quieter levels higher, so warnings and errors are at or below WARN.
    let lines = &log.lock().unwrap().lines;
    let errors: Vec<&String> = lines.iter().filter(|l| l.level <= tracing::Level::WARN).map(|l| &l.message).collect();
    assert!(errors.is_empty(), "{errors:?}");

    let world = &game.engine.world;
    let (spot, drift, gone) =
        (component(world, "EcsCheck.Spot"), component(world, "EcsCheck.Drift"), component(world, "EcsCheck.Gone"));
    assert_eq!(world.components().info(spot).layout().size(), 12, "a Vector3, as C# lays it out");

    // The cull's commands waited for its walk, then applied: the even half
    // is gone, the odd half tagged.
    let spots = with(world, spot);
    assert_eq!(spots.len(), 500);
    assert_eq!(with(world, gone).len(), 500);
    for &entity in &spots {
        let [x, y, z]: [f32; 3] = read(world, entity, spot);
        assert_eq!(x as i32 % 2, 1, "only odd X are left");
        assert_eq!((y, z), (FRAMES as f32, 0.0), "drifted up one a frame, through a span over the chunk");
        assert!(world.has_id(entity, drift) && world.has_id(entity, gone));
    }

    // The Obj got a component from C#, and a system wrote its Transform.
    let spinner = world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, name)| name.0 == "Spinner A").unwrap().0;
    assert_eq!(world.get::<Transform>(spinner).unwrap().scale.x, 2.0);
    let [speed]: [f32; 1] = read(world, spinner, component(world, "EcsCheck.Spin"));

    // Spots are written every frame; Drifts only when they were made.
    let tally = with(world, component(world, "EcsCheck.Tally"));
    let [frames, changed_spots, changed_drifts]: [i32; 3] = read(world, tally[0], component(world, "EcsCheck.Tally"));
    assert_eq!(frames, FRAMES);
    assert_eq!(speed, FRAMES as f32, "Get and Set on the Obj's component");
    assert_eq!(changed_spots, 500);
    assert_eq!(changed_drifts, 0, "nothing wrote a Drift since the tally last ran");
}
