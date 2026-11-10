# Voxear architecture

Voxear is a modern game engine with a data-oriented core: a Rust runtime, a Vulkan renderer, C# scripting on
CoreCLR, and a Unity-style editor whose look borrows from Unity's dark editor concept and from Linear. This document
records what the engine is, the decisions behind it, and how each system is meant to work. The plan for building it
is in [ROADMAP.md](ROADMAP.md).

## Principles

1. **Data-oriented core, friendly surface.** Everything is an entity in an archetype ECS (like DOTS). On top of that
   sits a familiar authoring model of Objs, Behaviours, scenes and Definitions, so nobody has to think in chunks
   unless they want to.
2. **Easy API and advanced API, for every system.** The easy path covers about 80% of needs in one or two lines and
   looks good with default settings. The advanced path exposes the real machinery. Both drive the same
   implementation, so moving from one to the other never means starting again.
3. **Nothing is a black box.** Core systems are Rust, open, and registered in the same schedule user code uses. Any
   system can be replaced, reordered, or extended from Rust or C#.
4. **Deterministic order.** Lifecycle callbacks, systems, and stage order are all defined. "Script execution order"
   bugs should not exist.
5. **GPU first.** Anything wide and parallel runs on the GPU: culling, skinning, particles, terrain, voxelisation,
   motion-matching search for crowds, and acoustic ray tracing.
6. **UX is a feature.** The editor has to be pleasant to use. Interactions are fast and keyboard-first, there are
   small 100–180 ms animations, it is built with the same UI system games use, and every feature works out of the
   box without reading documentation.
7. **Linux and Windows, every GPU vendor.** NVIDIA, AMD and Intel on both. The baseline is Vulkan 1.3; mesh
   shaders and ray tracing are optional tiers.

## Names

| Unity | Voxear | Notes |
|---|---|---|
| GameObject | **Obj** | A `readonly struct` handle to an entity, so it never allocates. `Object` collides with `System.Object` in every file that uses `System`, which is the confusion Unity lives with. |
| MonoBehaviour | **Behaviour** | Lifecycle order is guaranteed (see below). |
| ScriptableObject | **Definition** | A data asset described in C#. |
| DontDestroyOnLoad | **Scene Persistence** | The `ScenePersistence` component or `obj.Persist()`. |
| Singleton pattern | **IInstance** | Implement the interface to get `Instance<T>.Current`, with an editor badge. |
| Addressables / AssetBundles | **Content** and **Bundles** | Built automatically; you only hold a `Ref<T>`. |
| Scenes | **Scenes** and **Levels** | A Level is a set of Scenes. |
| Script Execution Order | `[ExecutionOrder(n)]` | |
| Prefab | **Prefab** | Nested prefabs and variants are planned. |
| UI Toolkit | **Voxear UI** | One system for the editor and for games, in screen space and world space. |
| Input System | **Input** | Schemes, rebinding and prompts are built in. |
| Localization package | **Localization** | String and asset tables, CSV import from a file or URL, live text binding. |
| Cinemachine | **Camera Rigs** | Simple by default. |
| Timeline | **Timeline** | |
| Shuriken / VFX Graph | **Particles** | One system with three levels of depth. |
| (none built in) | **Mods** | Content, sandboxed-script and full-trust tiers, with Steam Workshop built in. |
| Unity Hub | **Voxear Launcher** | Separate repository; installs engine versions and creates projects. |

## Layers

```
 ┌──────────────── Editor ─────────────────┐ ┌─── vox CLI ───┐ ┌─── Player ───┐
 │ panels · gizmos · undo · play mode      │ │ build · run   │ │ shipped game │
 │ C# extensions · debug tools             │ │ inspect · RPC │ │              │
 └─────────────────────────────────────────┘ └───────────────┘ └──────────────┘
 ┌──────────────────────── voxear_engine: frame loop ─────────────────────────┐
 │ Scenes/Levels · Content · Input · Audio · Animation · Physics · Navigation │
 │ Particles · Terrain · UI · Cameras · Timeline · Networking · Mods · Stores │
 │  Profiler · frame debugger · memory tools · debug draw · dev console       │
 ├─────────────────────── voxear_render: render graph ────────────────────────┤
 │  GPU culling · LOD · lighting · shadows · voxel GI · fog · post · text     │
 ├────────────┬──────────────┬───────────────┬──────────────────┬─────────────┤
 │ voxear_rhi │ voxear_ecs   │ voxear_scene  │ voxear_scripting │ voxear_core │
 │ Vulkan/ash │ archetypes   │ Obj+lifecycle │ CoreCLR host     │ math, time  │
 └────────────┴──────────────┴───────────────┴──────────────────┴─────────────┘
```

### Repository layout

```
crates/            Rust engine (one crate per system)
  voxear_core      math (glam), time, logging
  voxear_ecs       archetype ECS, queries, schedule
  voxear_scene     Obj components, behaviour lifecycle
  voxear_rhi       Vulkan device, swapchain, frame pacing
  voxear_assets    asset database, bundles and the content catalog
  voxear_render    renderer and passes, shaders/
  voxear_ui        Voxear UI: element tree, layout, text and the draw list
  voxear_scripting CoreCLR hosting and the interop tables
  voxear_engine    frame loop, projects, windowed runner
  voxear_cli       the `vox` binary
managed/           C# side: Voxear.Core (the scripting API), more to come (Editor, Generators)
samples/           example projects (`vox run samples/Hello`)
docs/              this document and the roadmap
manual/            the Voxear Manual: Markdown pages VoxDocs shows in every project
```

## Technology decisions

| Area | Choice | Why |
|---|---|---|
| Core language | Rust (2024 edition) | Speed and memory safety without a GC; good SIMD and threading. |
| GPU API | Vulkan 1.3 on `ash` | Direct control with nothing hidden; one API for Linux and Windows. |
| Required Vulkan features | dynamic rendering, synchronization2, timeline semaphores, buffer device address, descriptor indexing | Bindless, GPU-driven rendering. Supported by NVIDIA Maxwell+, AMD GCN+ and Intel Gen9+ on current drivers. |
| Optional tiers | mesh shaders, ray query / acceleration structures | Detected at startup (`vox doctor` shows them). Compute fallbacks always exist. |
| GPU memory | gpu-allocator | Proven allocator. It can be replaced later without API churn. |
| Shading language | **Slang** | Khronos-hosted. Modules, generics and interfaces make shader graph output and permutations clean. It compiles to SPIR-V today and other targets later. The bootstrap pass is GLSL until the material system lands. |
| Windowing | winit | Handles Wayland, X11 and Win32. |
| Scripting | C# on **CoreCLR**, .NET 10 LTS now, .NET 11 when it ships (November 2026) | The engine hosts `hostfxr` itself, with no `nethost` download. `Voxear.Core` is built with `RollForward=LatestMajor`, so newer runtimes work. |
| Shipping scripts | CoreCLR now; NativeAOT as an option later | NativeAOT gives faster startup and no JIT for players and consoles. Hot reload stays CoreCLR-only. |
| Physics | **Jolt** (C++, MIT) behind a Rust API | It is the fastest and most robust open option (Horizon Forbidden West, Godot 4). It is wrapped so that it is not a black box to users. |
| 2D physics | Box2D v3 (C, MIT) | The modern solver is fast and deterministic, and it pairs naturally with Jolt. |
| UI | **Our own retained UI** on taffy, parley, swash and cssparser, rendered by our Vulkan renderer | See [UI](#ui-voxear-ui). |
| Audio | Rust mixer on cpal, with spatial acoustics from **Steam Audio** (Apache-2.0) | Steam Audio provides HRTF, occlusion, transmission, reflections and pathing around corners. A GPU ray-traced path can come later. |
| Mesh import | glTF 2.0, and FBX through **ufbx** (C, MIT) | Both become the same prepared model, so LODs, cooking and previews are shared. meshoptimizer handles meshlets, LOD and vertex cache optimisation. |

## Conventions

- **Coordinates match Unity:** left-handed, **Y up**, **Z forward**, X right, units in metres. Positive rotation
  follows the left-hand rule, so a positive yaw turns to the right. Scripts and scenes ported from Unity keep their
  numbers.
- Rotations are quaternions. Euler angles in the inspector are in degrees and applied in Z, X, Y order, as in Unity.
- Importers convert right-handed formats such as glTF and FBX so that assets face +Z, and nothing is flipped at
  runtime.
- FBX files (`voxear_render::fbx_import`) come in metres, scaled by ufbx, and their axes are turned into glTF's in
  the vertices, each node's transform conjugated to match, as Unity's Bake Axis Conversion does: a Z-up model made
  in centimetres stands up at its size, with no -90 degree turn left on its Objs. A mesh splits into a part per
  material (`#mesh<mesh>.<part>`, `#material<index>`, in the file's order). Textures are read from the file or found
  beside it by name, and one found nowhere is left out with a warning.
- The renderer uses left-handed projection with reverse-Z depth (1 near, 0 far) for precision, and handles Vulkan's
  Y-down clip space internally.

## ECS and the Obj model *(implemented)*

**Storage.** `voxear_ecs` is an archetype ECS with 16 KiB chunks, like DOTS. Each archetype stores its components
structure-of-arrays inside chunks, and rows stay densely packed: every chunk except the last is full. Queries hand
out whole slices per chunk (`for_each_chunk`), which is the shape SIMD code and jobs want. Each chunk keeps a change
version per column, so `changed::<T>(since)` filters work per chunk.

**C# components share the same memory.** Components can be Rust types or plain-old-data registered by name and
layout at runtime. That is how a C# `unmanaged` struct lives in the same chunk memory: C# systems get a
`Span<T>` over a column with zero copies.

**Obj.** An Obj is an entity with `Name` and `Transform`. Behaviours are managed C# objects that the `Behaviours`
component points at. `InstanceMarker` and `ScenePersistence` are ordinary tag components, so the editor badge and
level persistence are just queries.

```csharp
// Easy: a Behaviour, exactly like Unity.
public sealed class Spinner : Behaviour
{
	protected override void Update()
	{
		Obj.Rotation *= Quaternion.CreateFromAxisAngle(Vector3.UnitY, Speed * Time.Delta);
	}
}

// Advanced: a C# system over chunk spans, no per-object calls.
public struct Spin : IComponent { public float Speed; }

public sealed class SpinSystem : SystemBase
{
	protected override void Update()
	{
		foreach (var chunk in Query<Transform, Spin>())
		{
			var transforms = chunk.Write<Transform>();
			var spins = chunk.Read<Spin>();
			for (var i = 0; i < chunk.Count; i++) { /* ... */ }
		}
	}
}
```

### The ECS from C# *(implemented in M3)*

The advanced path beside behaviours, for things there are hundreds or thousands of. It is in `Voxear.Core`
(`Entities/`), and the engine side is `crates/voxear_scripting/src/ecs.rs`.

- **Components** are `unmanaged` structs marked `IComponent`. Each is registered by its full name, size and
  alignment the first time it is used, and lives in the chunks beside Rust's components. `Transform` is one too, as
  the engine lays it out, so systems move Objs. Scripts may read any component that is plain data, write their own
  and `Transform`, and add or remove only their own: the engine's other components keep rules scripts cannot see.
- **Entities.** `Entity.Create()` makes one at once, even inside a system. An entity has `Has`, `Get`, `TryGet`,
  `Set`, `Add` (which chains), `Remove` and `Destroy`. An Obj has the same, through `obj.Entity`, so a behaviour can
  hand an Obj to a system with `Obj.Add(new Spin { Speed = 90f })`. Destroying an entity that is an Obj destroys it
  as `Obj.Destroy` does. Plain entities do not belong to a scene, so a level change leaves them.
- **Systems** derive from `SystemBase`. The engine makes one of each class in the game's scripts and runs its
  `Update` once a frame in its stage (`[UpdateIn(Stage.FixedUpdate)]`; Update by default), ordered by
  `[ExecutionOrder]` and then by name. Each stage's C# systems run after the behaviours' callbacks there and before
  the engine's own systems, so a Transform written in `PostUpdate` shows the same frame. Each system is a sample in
  the Profiler. They are made again, with `OnCreate`, each time play mode starts (the world is new) and each time
  the scripts reload, after `OnDestroy`.
- **Queries.** `Query<Transform, Spin>()` in a system, or `World.Query<…>()` elsewhere, fetches up to eight
  components, with `With`, `Without` and `Changed` (per chunk, since the system last ran). Walking it hands out each
  chunk, with `Count`, `Entities`, `Read<T>()` (a `ReadOnlySpan`) and `Write<T>()` (a `Span`, which marks the chunk
  changed). The engine writes the chunks' places into a pooled buffer once per walk, so the walk itself makes no
  calls into the engine, and building a query allocates nothing. `ForEach((ref Transform t, ref Spin s) => …)` is
  the short way, for up to three components. Objs the editor makes for itself, and Objs being destroyed, are never
  matched.
- **Commands.** Adding, removing and destroying move rows between chunks, which would leave a span pointing at the
  wrong row, so while a system runs or a query is walked they wait, and apply in order once it is done. Reading and
  writing a value in place happens at once.
- **Worlds.** Component ids belong to one world, and play mode makes a new one each time. The engine shares the id
  of the world it lends with scripts, which they read in place, so each component type registers again in a new
  world for the cost of one comparison per use.

- **Scenes, prefabs and the Inspector.** A scene or prefab gives an Obj components by type name, beside its
  behaviours, with their values in the same `fields` tables: `components = ["Game.Spin"]` and
  `[obj.fields."Game.Spin"] speed = 90.0`. They are kept and edited as behaviours are (a spec with a `component`
  flag), so prefab overrides, locks and undo work for them too, and ScriptInfo describes `IComponent` structs as
  `components` beside the behaviours, so the Inspector shows their fields, and Add Component lists them with the
  behaviours. When the game runs, each Obj's components are made from their values before any of its behaviours
  wakes, Obj references in them included, and `SceneComponents` keeps what was given for the Inspector and saving.
- **Live values.** While playing, the Inspector shows a component's values as the game has them now, and edits
  write into the game, where a behaviour's show what it started with. ScriptInfo works out where each field lies in
  the struct's bytes, as the runtime lays out a sequential struct, and gives the layout (offset, size and sign)
  only when it comes to the struct's real size; the editor reads and writes those bytes in place (`live.rs`).

Still to come: parallel jobs over chunks.

### Lifecycle guarantees *(implemented)*

Unity interleaves Awake and OnEnable per object and gives no order between objects loaded together. Voxear
guarantees four things instead (`crates/voxear_scene/src/lifecycle.rs`):

1. Every behaviour gets **Awake → OnEnable → Start**, in that order, once each.
2. Behaviours that arrive together (a scene load, an instantiated hierarchy) form a **batch**. Awake runs on the
   whole batch before OnEnable runs on any of it.
3. **No Start runs while anything is still waiting for Awake or OnEnable**, so Start always sees a fully initialised
   world. Objs created inside Awake finish their own Awake and OnEnable before any Start in the frame.
4. Within a phase, order is `[ExecutionOrder]`, then creation order (hierarchy order for scenes).

**Instantiate behaves like Unity's.** `Instantiate` and `AddBehaviour` run Awake on the whole new batch, then
OnEnable on it, before they return, so the caller gets an initialised object to configure. Start waits for the next
flush, so guarantee 3 still holds. The native side invokes Awake and OnEnable on the new batch directly, then hands
the lifecycle the batch as already awake and enabled. That way the lifecycle is never re-entered in the middle of a
dispatch. *(Planned with prefabs in M2; today behaviours are only created by the engine at startup.)*

The engine also skips phases a type does not override, so empty callbacks cost nothing. Enable, disable and destroy
requests made by scripts are applied between phases. A destroyed behaviour gets no further updates, then OnDisable
and OnDestroy at the end of the frame.

The frame order is fixed: `First` systems → lifecycle flush → fixed steps (FixedUpdate, then `FixedUpdate` systems) →
Update → `Update` systems → LateUpdate → `PostUpdate` systems → deferred destroys → `Last` systems. A flush runs
after each step.

### Instances (IInstance) *(implemented)*

```csharp
[ExecutionOrder(-100)]
public sealed class GameManager : Behaviour, IInstance { public int Score; }

Instance<GameManager>.Current.Score++;   // anywhere; cached, costs a compare per access
```

The instance is registered when it is created, before any Awake runs, so every Awake can already use it. A
duplicate logs a warning naming both Objs. The Obj gets an `InstanceMarker`, which the hierarchy shows as a badge. A
source generator will add the `GameManager.Instance` shorthand. A planned `[Instance(AutoCreate = true, Persistent =
true)]` attribute covers the common "manager that always exists" case.

### Math helpers *(implemented)*

Scripts use .NET's `System.Numerics` types, `Vector2`, `Vector3`, `Quaternion` and `Matrix4x4`, so they work with
any .NET library. Voxear.Core adds Unity's helpers onto those same types with C# 14 extension members. They read as
if they were built in, with no second `Vector3` type to convert to and from:

```csharp
Obj.Position = Vector3.MoveTowards(Obj.Position, goal, speed * Time.Delta);
Obj.Rotation = Quaternion.RotateTowards(Obj.Rotation, Quaternion.LookRotation(goal - Obj.WorldPosition), 90f * Time.Delta);
var hit = Obj.Forward * range;                       // world-space +Z, like transform.forward
var spot = Obj.WorldPosition + Random.InsideUnitSphere * 2f;
if (Random.Chance(0.1f)) { /* ... */ }
```

- **Vector3:**
  - directions: `Forward`, `Back`, `Up`, `Down`, `Right`, `Left`
  - movement and smoothing: `MoveTowards`, `SmoothDamp`, `Damp`
  - angles: `Angle`, `SignedAngle`
  - projection: `Project`, `ProjectOnPlane`, `ClampMagnitude`
  - on a value: `.Magnitude`, `.SqrMagnitude`, `.Normalized` (zero for a zero vector, where `Normalize` gives NaN),
    `.WithX/Y/Z`
- **Vector2:** the same set where it applies, plus `Perpendicular`.
- **Quaternion:**
  - `rotation * vector`
  - `Euler(x, y, z)` in degrees, Z then X then Y like scene files
  - `AngleAxis`, `LookRotation`, `FromToRotation`, `RotateTowards`, `Angle`
  - on a value: `.EulerAngles` (0 to 360, as Unity), `.Forward`, `.Right`, `.Up`
- **Mathf:**
  - `Clamp`, `Clamp01`, `Lerp`, `LerpUnclamped`, `InverseLerp`, `Remap`, `SmoothStep`
  - `MoveTowards`, `Repeat`, `PingPong`
  - angles: `DeltaAngle`, `LerpAngle`, `MoveTowardsAngle`
  - smoothing: `SmoothDamp`, and `Damp`, which smooths the same at any frame rate with no velocity to keep
  - `Approximately`
- **Random:** extensions on `System.Random`, so there is no second `Random` to clash with.
  - `Random.Range` (float, or int with max excluded), `Value`, `Chance`
  - `InsideUnitSphere`, `OnUnitSphere`, `InsideUnitCircle`, `Rotation`
  - For repeatable results, a seeded generator has the same helpers as `Next…`: `new Random(42).NextFloat(0f, 10f)`.
- **Obj:**
  - `Position` and `Rotation` are local, like Unity's localPosition.
  - `WorldPosition`, `WorldRotation`, and `Forward`/`Right`/`Up` are in world space, through every parent, and up to
    date mid-frame.
  - `LookAt(target)`.

`LookRotation` is Voxear's own, because the look-at helpers in `Matrix4x4` are right-handed and would face the wrong
way. Their tests, in `managed/Voxear.Core.Tests`, check Unity's conventions: a positive yaw turns forward to the right,
and a positive pitch looks down.

### The floating origin *(implemented across the ground, networked too; shader patterns and height planned)*

Positions stay f32 everywhere: the C# `Transform` is written in place, Jolt is built single-precision and the GPU
works in floats. Instead the world's middle follows the camera. `WorldOrigin` (a resource, whole metres as `I64Vec3`)
says where the world's middle is from the level's; a position in the world plus it is the position from the level's
middle, exactly, in f64. In a project that switches it on (`[floating_origin]`, Project Settings ▸ World: Off,
Endless Worlds or Every Level), once the viewer terrain streams round (`terrain::viewer`) is past its distance
(2048 m) from the middle on x or z, `origin::due` asks for a move of whole steps
(1024 m, a whole number of tiles, chunks and every world-anchored GPU grid), and `Game::move_origin` makes it at the
top of `Game::tick`, before anything that frame reads positions:

- `voxear_scene::move_objs` moves root Objs and every `WorldTransform`. A root not moved since the last move is put
  back from its place kept in f64, so a prop left 100 km behind returns bit-exact; scenes loaded after a move place
  their roots from the level's middle (`place_at_level`).
- `origin::shift` moves the rest of the world's state: tick smoothing's poses, `TerrainViewer`, physics
  (`PhysicsScene::shift`: one `vx_world_shift` moves every body without waking it and every character, then
  refreshes its contacts; kept places, terrain and block corners and plant models follow, rounding-sized differences
  absorbed; joints held to the world are held to a still anchor body with an empty shape), smokes, mists, motion
  matching, first-person eyes, gun holders and debug draw.
- `GameAudio::shift` moves where sources and the listener were and every 3D voice in the mixer at once
  (`Command::Shift`). The acoustics scene stays in the level's frame: walls, listener and voices get the origin
  added, so a move never rebuilds it and baked paths still fit.
- `Renderer::shift_origin` moves each view's last-frame view-projection (`· T(-by)`), model matrices (`T(by) ·`) and
  camera, so motion vectors and every reprojection see nothing move; DDGI cascade origins, track memory, foam and
  sea-floor origins move when the move is a whole number of their grids, else start again; the foliage cache's
  instances move rather than rebuild. The sea's cascades share no period, so `GpuOcean::phases` and
  `OceanSampler::set_origin` phase each by the origin, worked out in f64.
- Scripts hear last (`ManagedApi::origin_moved`): `[WorldPosition]` fields of live behaviours move, then
  `Origin.Moved` is raised. `Origin.Position`, `ToLevel` and `FromLevel` convert with `LevelPosition` doubles.

Ground streamed round the viewer counts its tiles and chunks from its own Obj, which moves with the world, so a move
changes no window and makes nothing again. Saves (`SceneState` copies, `SavedTransform`) keep roots from the
level's middle; a level load moves the world back to its own middle first. Edit mode never moves; when play stops
the Scene camera and kept values come back measured from the level's middle. Moves are on x and z only, as the
atmosphere and fog measure height from the level's ground.

Networked, a server and a host keep the level's middle (their world is every player's) and each client moves its
own (`net::origin_may_move`). Positions go between them measured from the level's middle: `write_state` adds the
sender's origin to a root Obj's position and `read_state` takes the reader's away, exactly, in i64 steps of
1/1024 m, so no machine needs another's origin. Each position carries a root bit (a child's stays its parent's)
and a far bit: within 8 km of the level's middle 24 bits an axis as before, further 16 bits an axis more. A
client's move shifts its remote samples, spawns waiting and prediction history (`net::shift`). C#: a Vector3 on a
`[Replicated]` property or an RPC argument marked `[WorldPosition]` is written as `Origin.ToLevel` doubles and read
through `Origin.FromLevel` (`NetTypes.WorldPosition`), its backing field marked so moves shift it. A server's own
physics stays f32 from the level's middle, so its Objs more than about 10 km out are less exact.

## Scenes, Levels and Scene Persistence *(implemented)*

- A **Scene** is a `.scene` TOML file (diff- and merge-friendly) holding Objs and their components. Each Obj has
  an `id` that is unique in the file, and `parent` refers to it. Rotations are Euler degrees, applied the way Unity
  applies them. Components are subtables:
  - `[obj.camera]`, `[obj.light]` and `[obj.renderer]`, the last with a mesh reference and a material reference or
    inline material
  - `behaviours = ["Game.Spinner"]`, and `components = ["Game.Spin"]` for C# components; `[obj.enabled]` lists
    behaviours unticked in the Inspector, as `"Game.Spinner" = false`, which start disabled
  - `persistent = true`
  - a scene-level `[sky]`
  See `crates/voxear_engine/src/scene_file.rs`. Builds will use a binary form made by the content build.
- A **Level** is a `.level` file listing the Scenes it needs, as CookieJarTools' **LayerCake** loads them
  *(implemented)*. Its source and guide are in `Dev/CookieJar` (`Assets/CookieJarTools/LayerCake`,
  `Docs/LayerCake.md`). Each scene may be marked:
  - `active = true`: where new Objs go, and whose sky and post-processing the level uses. Unmarked, a level's first
    own scene is active; only a level with none of its own takes its base's.
  - `required = false`: a scene that fails to load is passed over rather than stopping the change.
  - `weight = 3.0`: what it counts for in a background load's progress, against the others.
  - `reload = true`: reloaded even when the previous level had it too.

  A level can **stand on** another (`stands_on = "Levels/World.level"`) for the scenes that are always there:
  managers, audio, the persistent UI. The base's scenes come first and stay loaded while the levels standing on it
  give way to each other, so nothing in them is torn down and built again. Hello's Day and Night stand on World,
  which holds the Environment.
- `vox assets` lists a project's assets and gives new ones a GUID.
- `Levels::load(level)` diffs the scenes loaded now against the scenes the level needs:
  - Scenes shared between the two levels **stay loaded** unless marked `reload`.
  - Missing scenes load.
  - Scenes the new level does not need unload (unless `LevelOptions::unload_rest` is off), and their content
    refcounts release.
- Sync and async are a choice per call *(implemented)*:
  - `Level.Load(level)` changes level at the end of the frame, reading everything then.
  - `Level.LoadAsync(level, options)` loads in the background while the game runs, then changes over at the end of
    the frame it finishes in, as LayerCake's `SceneLoader.Load`. It returns a `LevelLoad`, the change as it happens:
    - `Progress`, from 0 to 1 across every scene, each counting for its weight and scenes already loaded counting as
      done; it reaches 0.9 when loaded and 1 when changed over, as in Unity
    - `IsReady`, `IsDone`, `Error`, and the `Scenes` it brought in
    - awaiting it carries on once the level is in, and throws `LevelLoadException` if the change failed
    - `LevelLoadOptions`: `Straight` or `Held` (wait at `IsReady` until `Activate()`), `LeastTime` (a change takes at
      least this long, so a loading screen is seen), `UnloadRest`, `SetActive`, `TidyUp` (free what nothing uses
      once it is over) and `ReloadShared` (the scenes both levels have load again, as if each were marked `reload`);
      `LoadAsync(level, activate: false)` is the same as `Held`
  - **No handle needed** *(implemented)*: the change going on is `Level.Current`, so `Level.Progress` (1 when none
    is going on), `Level.IsReady`, `Level.Activate()`, `Level.Next` (the level coming in) and `Level.Error` read and
    drive it from anywhere, and a loading screen binds to them. They cost no more than reading a field. The same
    shape is meant for other APIs with one obvious thing in flight: the easy form reads it, and the handle stays for
    those who want it. Where the easy form costs more, its documentation says so.
  - **Single scenes** *(implemented)*, for games that want more say than levels give: `Scene.LoadAsync(scene,
    options)` loads one in the background alongside those loaded (`LoadTarget::Scene`, then `Levels::add_scene`),
    waiting at ready as a level does; `SceneLoadOptions` has `SetActive`, `Reload` and `Hold`.
    `Scene.ReloadAsync`, `Scene.Unload` and `Scene.SetActive` (both at the end of the frame), `Scene.Loaded` and
    `Scene.IsLoaded` (which ask the engine and make a list each time), and, as for levels, `Scene.Current`,
    `Progress`, `IsLoading`, `IsReady` and `Activate()`. A scene loaded this way raises `Level.SceneLoaded` too.
    The level stays the level: its next change unloads the scenes it does not want, these with them, unless that
    change has `UnloadRest` off.

    ```csharp
    var switching = Level.LoadAsync("Levels/Night.level", LevelLoadOptions.Held);
    await switching.Ready;            // everything is read in and waiting
    await FadeOut();
    switching.Activate();             // let it in
    await switching;                  // and it's there
    ```

  - **Events**, static so anything can listen without being handed anything: `Level.SwitchStarted`,
    `SwitchProgressed` (every frame while reading), `SwitchReady` (for a held change), `SceneLoaded`,
    `SwitchFinished` and `SwitchFailed`. `Level.Active` is the level loaded now, including one opened in the editor,
    `Level.ActiveScene` its active scene, and `Level.Current` the change going on. Only one change happens at a
    time; asking for another while one goes on gives back the one going, with a warning. Static events outlive a
    play-mode run in the editor, so listeners should unsubscribe in OnDestroy. When the scripts reload, listeners
    from their old build are dropped; listeners that subscribe in OnEnable and unsubscribe in OnDisable carry on,
    since a reload runs both.

  - **How it works.** A worker thread with its own reader works from the project folder or from bundles. It reads and
    parses every level, scene, prefab and material file the level needs, and prepares its models: parsing glTF,
    decoding images and building LODs. The main thread uploads the prepared models within a 4 ms budget per frame,
    then changes level from the texts already read, with no IO left to do. Anything already loaded is skipped. In
    Rust it is `Game::load_level_async`, `Game::load_status` and `Loads::set_allow_activation`; the code is in
    `crates/voxear_engine/src/loading.rs`.
  - Assets that behaviour fields name (`Prefab` and `Ref<T>` fields) are not preloaded, since they load on their
    own.
  - The editor adds and takes out a level's own scenes, and sets `active` and `required`, from the Inspector (see
    the Inspector for assets); `stands_on` and `weight` are still written by hand.
- **Scenes opened alongside** a level, additively, as Unity's Open Scene Additive (`Levels::open_scene_alongside`,
  by `AssetGuid`): the scene loads with the open ones, nothing unloads, and the active scene keeps its sky and
  post-processing. `Levels` remembers the scenes the level (or the scene opened on its own) asks for, so
  `opened_alongside` tells the others apart; opening the same level again, as the editor does after changing it,
  keeps them, and any other level or lone scene closes them. `Levels::close_scene` unloads one open scene,
  persistent Objs included in edit mode, and moves the active scene to the last one open when it was that one.
- **Scene Persistence:** Objs marked `persistent` (the `ScenePersistence` component) survive every level change,
  along with their children, like `DontDestroyOnLoad`. `obj.Persist()` will be the easy form in code.
  - **No duplicates on reload, unlike Unity.** Reloading the scene a persistent Obj came from does not create a
    second copy. The engine remembers which scene and id the survivor came from and skips it, so there is no "destroy
    myself if a copy exists" boilerplate.
  - **Only a running game keeps them.** In edit mode (`Engine::set_edit_mode`) persistent Objs unload with their
    scene like any other, so the editor opening another level, scene or prefab does not carry them along.
  - In the editor, the Inspector's **Persistent** box marks an Obj, as a step to undo, and saving writes it.
- From C#, `Level.Load("Levels/Night.level")` changes level after the current frame. Unloaded Objs stop rendering
  at once, and their behaviours get OnDisable and OnDestroy at the end of the next frame.
- Loading a scene is one lifecycle batch, so ordering guarantee 2 holds across the whole scene.
- **Objs created at runtime**, such as instantiated prefabs, belong to the level's active scene and unload with
  it, unless they are persistent. Objs marked `EditorOnly`, such as the editor's Scene camera, belong to no scene
  and level changes leave them alone.
- **The Levels panel** in the editor is LayerCake's window: every level (with its scene count and the level it stands
  on) and every scene in no level, to find by name, listed again when the Project changes files. Stopped, clicking a
  level opens it to edit (a loose scene opens on its own) and its play button opens it and plays. Playing, clicking a
  level changes to it through the scripts' `Level.LoadAsync`, so every event fires and the game behaves as it would in
  a build; the bar at the bottom follows any level change, whoever started it.

### Prefabs *(implemented: nesting, variants, overrides, Instantiate; inspector editing planned)*

A **Prefab** is a `.prefab` file: Objs written exactly as in a scene, with one root.

**Easy.**

```csharp
private Prefab enemy;

protected override void Awake()
{
	enemy = Prefab.Load("Prefabs/Enemy.prefab");     // once; loading again is free
}

protected override void Update()
{
	var copy = enemy.Instantiate(spawnPoint, Quaternion.Identity);
	copy.GetBehaviour<Enemy>()!.Target = player;      // its Awake and OnEnable have already run
}
```

`Instantiate` works like Unity's:
- Every new behaviour gets Awake, then OnEnable, before it returns, in batch order: Awake on all of them first, by
  execution order and then creation order.
- Start comes at the end of the current phase, before the next Update.
- `Instantiate(position, rotation)` places the root in world space and keeps the prefab's scale.
- `Instantiate(parent)` keeps the root's local placement under the parent.

Also from C#:
- `obj.AddBehaviour<T>()` runs Awake and OnEnable before returning, like AddComponent.
- `obj.GetBehaviour<T>()` and `TryGetBehaviour<T>(out ...)`.
- `obj.Destroy()` destroys the Obj and its children at the end of the frame.

In Rust:
- `Game::instantiate(path, parent, placement)`, or `Content::prefab` and then `Engine::instantiate` with a loaded
  template.
- `Engine::add_behaviour`.

**In scenes.** An Obj with `prefab = "Prefabs/Lamp.prefab"` is an instance:
- The prefab's root takes the instance's id and parent.
- It also takes whatever else the instance writes: name, position, rotation, scale, camera, light, renderer and
  `persistent`. Behaviours listed on the instance are added to the root's.
- `[obj.overrides."<path>"]` changes Objs inside the prefab, layering only the fields written, the same way
  `[post]` layers onto a preset. A path is an Obj's id in the prefab, so `2`, or `3/2` inside a nested prefab.
- Overrides cannot change `id`, `parent` or `prefab`, and typos are errors.

```toml
[[obj]]
id = 5
name = "Cyan lamp"
prefab = "Prefabs/Lamp.prefab"
position = [2.5, 0.6, -0.5]
light = { type = "point", color = [0.25, 0.85, 1.0], intensity = 6.0 }
[obj.overrides.2.renderer.material]
emissive = [1.25, 4.25, 5.0]
```

**Nesting and variants** need nothing extra:
- A prefab can contain instances of other prefabs. Expansion is recursive, and a prefab that contains itself is an
  error that names it.
- A **variant** is a prefab whose root is an instance of another prefab, carrying its overrides and any children it
  adds. Changes to the base flow through to every variant.

**Saving.** A scene saves each instance as an instance: what differs from the prefab is written as instance fields
and overrides, and Objs inside instances are not written on their own.
- Every Obj from an instance carries `FromPrefab`, which names the outermost instance and the Obj's path inside it.
  The instance root also carries `PrefabInstance`.
- Not saved yet: Objs deleted inside an instance, and cameras, lights or renderers removed from one. Both come back
  from the prefab on reload, until the editor adds "removed" overrides.

**Loading.** `Prefab.Load` expands the prefab once and loads its meshes and materials into a template. Every
`Instantiate` after that only creates entities, so spawning bullets costs no file or GPU work. `Prefab.LoadAsync`
loads in the background the same way levels do, and awaiting it gives the prefab:
`var boss = await Prefab.LoadAsync("Prefabs/Boss.prefab");`.

**Awaiting in scripts.** Voxear.Core installs a main-thread `SynchronizationContext`, as Unity does. A continuation
after any await (a load, `Task.Delay`, a web request) runs on the main thread at the start of the next frame, before
the `First` stage, where it can touch Objs. `async void` lifecycle methods work too: `protected override async void
Start()`. An exception thrown after an await is logged against the frame, never lost.

**Null dereferences.** Every entry point Voxear.Core gives the engine catches what C# throws and logs it, including
the `NullReferenceException` of a null dereference. On Linux and macOS .NET raises that one from inside its `SIGSEGV`
handler, on the thread's alternate signal stack. Rust gives its threads an 8 KiB stack, which that handler overflows,
so `voxear_scripting` gives each thread a 64 KiB one before its first call into C# (`signals.rs`).

### Behaviour fields *(implemented)*

Scenes and prefabs set behaviour fields, as Unity's inspector does. Values are written per behaviour type, so an
instance or override changes one field at a time:

```toml
[[obj]]
id = 11
name = "Fireflies"
behaviours = ["Hello.FireflySpawner"]
[obj.fields."Hello.FireflySpawner"]
firefly = "Prefabs/Firefly.prefab"        # a Prefab field: loaded with the Obj
rare_firefly = "Prefabs/RareFirefly.prefab"  # a Ref<Prefab> field: loaded when asked
rare_chance = 0.1
```

```csharp
public sealed class FireflySpawner : Behaviour
{
	[SerializeField] private Prefab firefly;
	[SerializeField] private Ref<Prefab> rareFirefly;
	[SerializeField] private float rareChance = 0.1f;
	public float Interval = 0.25f;                   // public fields need no attribute
}
```

**Which fields.**
- A field can be set when it is public, or private with `[SerializeField]`, and is neither readonly nor
  `[NonSerialized]`.
- Names match ignoring case and underscores, so scenes use snake_case like every other key.

**When.** Values are set just before the behaviour's Awake, which is when Unity has finished deserialising too. A
behaviour with field values always gets Awake dispatched, even if its type does not override it.

**Types:**
- numbers, `bool`, `string` and enums (by name or number)
- `Vector2`, `Vector3` and `Vector4`
- `Quaternion`: Euler degrees like transforms, or four components
- `Color`: hex as a design tool writes it, `"#ff8800"` or `"#ff880080"` with alpha, or three or four numbers from 0 to
  1. The Inspector shows a swatch that opens the colour picker, then red, green and blue to type; alpha keeps what the
  script or scene gave it. A C# component's `Color` is edited the same way while playing.
- arrays and `List<T>` of any of these
- **`Prefab`** and **`Sound`:** a direct reference, loaded with the Obj, before Awake.
- **Component fields:** a field of an engine component reached through an Obj, as `AudioSource` is (the internal
  `IObjComponent`), takes an Obj that has one. The catalog describes it as `{ kind = "component", name }`, the
  Inspector's picker offers only Objs with that component, and a scene saves the Obj, as for an `Obj` field
  *(implemented)*.
- **`LevelAsset`** and **`SceneAsset`:** a level or scene of the project, picked in the Inspector from the project's
  levels or scenes or dropped there from the Project, and handed to `Level.LoadAsync` or `Scene.LoadAsync`. They
  hold the GUID or path and read nothing until the level or scene loads, so a field never needs a path typed in a
  string *(implemented)*.
- **`Ref<T>`:** a lazy reference, like an Addressables AssetReference. It holds the path or GUID, and
  `rareFirefly.Load()` loads it when the game first needs it. `T` is any `IAsset<T>`, which `Prefab` and `Sound` are
  today.
- **`Obj`:** another Obj in the same scene or prefab, like dragging a GameObject into a field. It is written as its
  id or path, and arrays of them work too:

  ```toml
  lamps = [{ obj = 3 }, { obj = 5 }]   # Objs 3 and 5 in this file
  bulb = { obj = "9/2" }               # Obj 2 inside the prefab instance 9
  ```

  - **Inside a prefab**, references point at each instance's own copies: the Lamp prefab's `Pulse` names its bulb as
    `{ obj = 2 }`, and every lamp pulses its own. This holds through nesting and variants.
  - **References stay inside their file**, because nothing else is sure to be loaded with it (Unity has the same
    rule across scenes). A reference to an Obj that is not in the file is an error that names both.
  - **Saving** writes each reference back as an id or path in the saved file, following any renumbering. A
    reference to an Obj that is not being saved becomes `{}`, which loads as `Obj.None`.
- **Behaviours and interfaces:** a field of a behaviour type (`Spinner leader`) or an interface (`IDamageable
  target`) is written as the Obj it is on, `{ obj = 2 }`, and gets that Obj's first matching behaviour. Every
  behaviour in a scene or prefab exists before any Awake, so the reference is always there to find. In Hello,
  Spinner B follows Spinner A this way.
- **Nested types:** a `[Serializable]` class or struct is written as a table of its own fields, by the same rules,
  so lists of settings read naturally. Warnings name the exact value that did not fit, such as
  `GameManager on Game Manager: cycle[1]: Seconds`.

  ```toml
  fields."Hello.GameManager" = { cycle = [
      { level = "Levels/Night.level", seconds = 5.0 },
      { level = "Levels/Day.level", seconds = 5.0 },
  ] }
  ```

  A table with only an `obj` key, or only an `entity` key, is always read as a reference, so those names are
  reserved inside nested types.

**Mistakes are reported, never silent:**
- Field values for a behaviour the Obj does not have are an error that names the Obj.
- A field name the type does not have, or a value that does not fit, is a warning naming the behaviour, the Obj and
  the field. The field keeps its default.

**Saving.** Scenes save field values as they were loaded. Instances save only the fields that differ from the
prefab. Values that scripts change at runtime are not written back. The editor's inspector will edit the saved values
directly.

**Not yet supported:**
- Dictionaries, and polymorphic values (a field of a base class holding a subclass), which Unity's
  `[SerializeReference]` covers.
- A Roslyn suppressor for warning CS0649, which fires on `[SerializeField]` fields because the compiler cannot see
  the engine set them. Until it exists, game projects turn that warning off, as Hello's `.csproj` does.

## Saves *(implemented; pictures planned)*

A game's saves, from `Voxear.Save` in C#. The work is split where it is cheapest: C# knows the values and their
types, so it takes them apart and puts them back; Rust knows files, so it keeps the slots and writes them safely.

- **Slots** (`voxear_scripting::saves`, `SaveFolder`): each slot is one file, `<slot>.save`, in the game's save
  folder. The host sets the folder with `ScriptRuntime::set_saves`: a build uses `Saves` beside its `player.log`,
  under the company and product names of its profile (`player::save_folder`); the editor and `vox run` use the
  project's name without a company, which is where a build whose profile names neither lands too. Names are file
  names on Linux and Windows alike (letters, digits, spaces, `-`, `_`, `.` and brackets, no device names, at most 64
  characters), matched ignoring case on both, as Windows matches files.
- **The file:** a 40-byte header (`VOXSAVE\0`, the file format, when it was written, the three parts' lengths and
  a CRC-32 of them), then three parts: *info*, JSON that C# writes about the save (its format, its top type, its
  version and summary); *picture*, a PNG for load menus, empty until the engine takes one; and *data*. Listing slots
  reads only each header and info. Reading the data checks the length and the CRC, so a cut-short or changed file is
  refused as damaged rather than read as nonsense.
- **Writing:** the bytes go to `<slot>.save.writing`, are synced to disk, and are renamed over the slot's file,
  which replaces it in one step on both systems; on Unix the folder is synced too, so the rename itself lasts. A
  crash or a full disk at any point leaves the save before. Writes queue on one `save writer` thread and run in
  order, each with a ticket that C# polls; every read, list and change waits for the writes queued before it, so a
  script always reads what it last wrote, and dropping the `ScriptRuntime` waits for them all, so quitting right
  after a save keeps it.
- **Native calls** (API 28): `save_write`, `save_wait`, `save_read` (a part, with the file kept between the call
  that asks its length and the one that copies it, so it is read once), `save_change` (delete, copy, rename,
  exists) and `save_info` (the slots, the folder, the default format, the last failure's message). Failures are
  numbers below 0, one per `SaveError` kind. `save_key` and `save_obj` turn Objs and assets into keys and back
  through `ScriptServices::obj_key`, `obj_by_key` and `asset_guid`.
- **Values** (`Voxear.Saving`): C# turns values into a `SaveNode` tree (null, bool, integer, float, text, bytes,
  list, table), and a `SaveFormat` turns the tree into bytes: binary (`VSB` and a version byte, then every name and text once in a table at
  the start, then tagged values, zigzag varints, floats in four bytes when that is exact), the default in builds,
  or indented JSON, the default in the editor and `vox run`. The info names the format, so a save always reads
  back with the one that wrote it, and a game's own formats, found by type like converters, read the same way.
  Migrations and converters see the tree, never bytes, so they work the same in every format.
- **The walk** (`SaveWriter`, `SaveReader`): a converter for the type comes first (added in code, then found in
  the game scripts, then the engine's own for vectors, quaternions, colours, matrices, dates, times and GUIDs);
  then plain values (enums by name); then the engine's things (an Obj as `<scene GUID>:<ObjPath>`, from
  `Levels::place_of`, so it is found in a later run while its scene is open; a behaviour that another value points
  to as its Obj; any asset as its GUID, which every `IAsset` type gives through its internal `AssetGuid` (a
  `save_key` kind each: prefabs 2, sounds 3), and reads back through its `Load`; `Ref<T>` as the GUID of what it
  names); then dictionaries (a table when keys read as text, else
  `[key, value]` pairs), collections and arrays; and then a type's members: public fields, `[SerializeField]`
  private ones (readonly too), and public settable properties, init-only ones included, less `[NonSerialized]`,
  `[DontSave]` and delegates. A behaviour at the top of a save is its fields, read back with `ReadInto`. A value
  whose type differs from its field's records `$type`; a class reached twice is written once with `$id` and then
  as `$ref`, so sharing and cycles survive. Classes are made with their constructor that takes nothing, or without
  one for records, so members a save lacks keep their initial values. A value that does not fit warns, naming
  where it is, and keeps its initial value, so one bad value never costs a player the rest of the save.
- **Versions:** a type marked `[SaveVersion(n)]` writes `$version` in its table, and its static `[MigrateFrom(k)]`
  methods (or `Save.AddMigration<T>`) change a table from version k to k+1 before it is read, one step at a time,
  top-down, so a nested type migrates by its own version after its holder's. A table from a newer version than its
  type's is refused with `SaveError.NewerVersion`.
- **Easy and advanced:** `Save.Write(slot, data)` and `Save.Read<T>(slot)` need nothing set up; `Read(slot,
  fallback)` and `TryRead` never throw. `Save.Write` waits for the disk; `WriteAsync` takes the values apart at
  once and leaves only the disk to another thread, as a `SaveWrite` to await. `SaveFile` keeps several named parts
  in one slot, with its own format, summary and version, and `Save.Slots` reads only the headers.
- **Saves of the scene** (`SceneState`, `Save.WriteScene`, `Save.LoadScene`): the open scenes as the player
  left them, gathered from marks rather than a class of the game's own. `Capture` asks the engine (`save_scene`,
  `ScriptServices::scene_state`, API 29) for the level's GUID, the Objs scenes loaded that are gone
  (`Levels::gone`: their entity is dead, being destroyed, or waiting in the script call's destroys, or under one
  that is), and the root of each copy scripts made from a prefab: `spawn_prefab` gives every Obj of a copy
  `Spawned { prefab, root, index }`, its index in the prefab's order. Then it walks every live behaviour and keeps
  the members `[Saved]` marks (`SceneMembers`: marked fields and properties of any visibility, or, on a marked
  class, what `SaveMembers` keeps), each by its Obj's key, its class, and which of that class on the Obj it is.
  A copy's Obj has the key `spawned:<root entity>/<index>`, good for one run only, so the state numbers its
  copies and writes `spawned:<number>/<index>` (`SpawnedKeys`), in its own records and in any value pointing at
  one. `LoadAsync` loads the level again with `ReloadShared`, so every scene is fresh, then `Apply` destroys the
  copies there are, destroys the gone Objs, makes each copy again under its parent (a copy's parent first),
  placed as it was, and fills the marked members, with the numbers pointing at the new copies. The continuation
  runs before the reloaded scene's Awake, so its behaviours start with what was put back, and
  `Save.IsLoadingScene` stays true until the end of that frame, for spawners to skip their first spawn.
  `SavedTransform` is a behaviour with one marked property, its Obj's transform.
- **Planned:** C# components kept in a save of the scene; a picture of the game with each save; a store interface
  for cloud saves (Steam's, with the platform API); and a Saves window in the editor.

## Content (assets, Bundles, loading)

This is the only way to load assets. It replaces Addressables and fixes the build pain by making bundling automatic.

**Authoring.**
- Every asset has a stable GUID in a sidecar `.meta` file, for example `Ship.glb.meta`. Moving a file together with
  its `.meta` keeps its GUID, and a copied file and `.meta` get a fresh one *(implemented in `voxear_assets`)*.
  The project's own `Docs` folder, VoxDocs' pages and their pictures, is documentation rather than content, so
  its files are not assets and never ship.
- The `.meta` also keeps the asset's import settings, as Unity's do, in a table named for its kind: a model's
  `[model]` holds its units and scale, where its normals and tangents come from (the file's, or calculated by a
  smoothing angle), whether it gets LODs and uses its own materials; a picture's `[texture]` its largest size and
  whether a build compresses it, used where a model reads it. Only settings that differ from the defaults are
  written, and a copy keeps them. The importers read them through the same callback as the model's other
  files, so the editor, a game run from the project and a content build import alike, and a build's cook key
  covers them. Applied in the Inspector, they import the model again behind the mesh and material handles it had
  (`Renderer::reupload_model`), so every Obj, undo step and preview drawing it shows the change, and a placed part
  still where the model put it follows it *(implemented)*.
- Files come in as Unity's do *(implemented)*. One dropped on the editor's window from outside is copied into
  the folder under the pointer in the Project panel, or the folder it shows, numbered if its name is taken, with
  a `.meta` dropped beside it following it. winit passes drops on X11, Windows and macOS but not on Wayland, so
  there the editor binds a data device for each seat on winit's own connection, in an event queue of its own, takes
  drags carrying `text/uri-list`, and hands the window the drag's position as the pointer moving and the files as
  dropped files (`wayland_drops.rs`, with smithay-client-toolkit, which winit already uses). When the window gets the
  focus back the editor compares the project's
  files with when it last looked: files that came or went show in the Project panel, and a model in use whose
  file, `.meta`, buffers or pictures changed is imported again. With the same parts it goes behind the handles it
  had; with others, as after a re-export that added a mesh, it is uploaded anew, the Objs drawing it move to the
  new parts by name, and the old model stays loaded for the undo steps that may still draw it.
- Files reference assets by GUID, optionally with a part inside the asset: `9f2c…#mesh0.0` is a mesh inside a model.
  Hand-written files may use project paths instead, which are resolved on load.
- In code, a field of type `Ref<Mesh>`, `Ref<Material>`, `Ref<Level>` and so on is all a developer writes. The
  editor's object picker fills it in.

```csharp
[SerializeField] private Ref<Prefab> enemy;             // set in the scene as a path or GUID

var copy = enemy.Load().Instantiate(spawnPoint, Quaternion.Identity);  // implemented: loads on first use
var mesh = await meshRef.LoadAsync();                     // planned: async
using var handle = Content.Load(meshRef, Priority.High);  // planned: explicit lifetime and priority
```

**Content build** *(implemented: steps 1–3, 5, and cooking models with compressed textures; step 4 planned)*. It runs alone
with `vox content build`, before `vox run --bundles` and `vox render --bundles`, and will run before every player
build. Its code is in `crates/voxear_engine/src/content_build.rs`, and the bundle format and reader are in
`crates/voxear_assets/src/bundle.rs`.

1. **Walk the dependency graph from every root.** Roots are what a game can ask for by name: every level, every
   prefab, every UI document (`.vui`), and every asset a behaviour field names.
   - A field's asset is a root of its own because `Ref<T>` loads it separately. A `Prefab` field is treated the same
     way, so its bundle opens at the Obj's Awake.
   - Dependencies come from the files: a level's scenes, the prefabs, meshes and materials that scenes and prefabs
     use (overrides included), and the textures a model names. Files a model reads that are not assets, such as its
     `.bin` buffers, are packed with it, and so are the `.vss` sheets a UI document names, which it reads by path in a
     built game as it does from the project.
   - `[content]` in `voxear.toml` adjusts the roots with path patterns (`*` within a folder, `**` across
     folders):

     ```toml
     [content]
     include = ["Textures/Icons/**"]   # loaded by paths the build cannot see
     exclude = ["Prefabs/Debug/**"]    # test and debug content stays out of builds
     ```

     Excluding stops an asset being a root of its own. One that a shipped scene still uses, or names in a behaviour
     field, ships anyway, so a build never loses what it needs.
   - **`[ContentRoot]`** *(implemented)* names what code loads by a path the build cannot see. It takes paths,
     GUIDs or the same patterns, and goes on a class, struct, method, field, property or the assembly:

     ```csharp
     [ContentRoot("Prefabs/Bosses/**")]
     public sealed class BossSpawner : Behaviour
     {
         private void Spawn(string boss) => Prefab.Load($"Prefabs/Bosses/{boss}.prefab").Instantiate();
     }
     ```

     Like a field naming an asset, a content root ships even when `exclude` matches it. The build loads the game's
     scripts to read the attributes, lists each root with the type or member that declares it, and warns about a
     pattern that matches no asset, which is usually a typo.
2. For each asset, compute its **usage set**: the roots that can reach it. Assets no root reaches are left out, and
   the build lists them.
3. **Group assets by identical usage set.** Each group is a bundle. Loading any root then loads exactly the bundles
   whose usage set contains it, and nothing it does not need. Shared assets between levels end up in their own
   bundles automatically. In Hello, the Day level opens its own bundle and the Environment bundle it shares with
   Night. Night opens its own, the shared one is already open, and the firefly bundle opens only when the spawner
   loads its prefab.
4. **Refine the sizes.**
   - A group over `split_above_mb` (64 MB by default) splits into several bundles with the same roots, so a patch or
     download touches less. Assets themselves never split.
   - A bundle under `merge_below_kb` (16 KB by default) merges into one whose roots include all of its own.
     Whenever any of its roots loads, that bundle opens anyway, so the merge saves a file at no cost. It picks the
     host with the fewest roots, then the smallest. Hello goes from 7 bundles to 3 this way.
   - Both limits are set under `[content]` in `voxear.toml`, and 0 turns merging off.
   - *Planned:* large streamable data such as texture mip tails and audio streams in bundles of their own, so they
     stream independently.
5. **Write bundles and a catalog.**
   - A bundle is a header followed by each file, compressed when that saves space: with LZ4, quick to load, by
     default, or Zstd, smaller, or not at all (`[content] compression`, which a build profile can change; the
     catalog notes each file's codec). Cooked models are stored as
     they are, since they compress their own GPU data (below). A bundle is named after a hash of
     its bytes, so a rebuild that changes nothing writes nothing, and bundles no longer listed are deleted.
   - The **catalog** (`catalog.bin`) maps each asset's GUID and path to its bundle, offsets and sizes. It also
     lists each bundle's usage set and the bundles it depends on. It is TOML inside Voxear's own wrapping
     (`crates/voxear_assets/src/packed.rs`): Zstd, then XORed with a SplitMix64 keystream salted by the bytes'
     own hash, so no path can be read from a build and an unchanged catalog wraps the same. `vox content build`
     prints what it holds. A `catalog.toml` from an older build still reads.
   - **Nothing ships in its source format**, as asset licences usually ask. Models and pictures are cooked
     (below), sounds keep their encoded stream inside the same wrapping (a WAV is LZ4- or Zstd-compressed
     first), and only development builds may store bundles uncompressed (`compression = "none"` becomes LZ4
     otherwise). This is obfuscation, not encryption.
   - **Models are cooked**: stored ready for the GPU in Voxear's own format (`crates/voxear_render/src/cooked.rs`).
     - A cooked model holds its meshes with their LOD chains, its textures, its materials and its node layout.
     - **Its GPU data is one payload**: every vertex, index and texture mip, 16-byte aligned, compressed with
       **GDeflate** in 64 KB tiles that each decompress on their own. The GPU decompresses all of a model's tiles
       at once where the driver offers `VK_NV_memory_decompression` (`vox doctor` lists it as GPU decompression),
       and the loading thread decompresses them otherwise. Data that does not compress ships raw. The encoder is
       Microsoft's GDeflate code from DirectStorage, through the `gdeflate` crate. The cross-vendor
       `VK_EXT_memory_decompression` takes the same data, and follows once ash has bindings for it.
     - **Textures are compressed**, to a quarter of RGBA8's GPU memory and bundle size: BC7 for colour, and BC5 for
       normal maps. Shaders rebuild a normal's Z from X and Y, so BC5's two channels are enough. Mips are built on
       the CPU before compression, averaged in linear space so they do not darken. Textures whose sides are not
       multiples of 4 stay RGBA8. The encoders (`crates/voxear_render/src/texture_codec.rs`) are Voxear's own and
       simple: BC7 uses mode 6 with endpoints fitted along each block's principal axis, then refined by least
       squares. Hello's textured pyramid renders within 13 levels of its source (71 dB).
     - Loading one is a read and an upload, with no glTF parsing, image decoding or LOD building at runtime, in
       `Content` and in background loads alike. The upload is **one submission**: decompress the tiles into a
       scratch buffer, then copy every mesh into the shared buffers and every mip into its texture. RGBA8 textures
       carry their mips too, made at cook time, so nothing is generated on load. GPU and CPU decompression give
       byte-identical frames.
     - The model's buffers and images are baked in, so they do not ship separately.
     - Cooks are cached in `.voxear/ContentCache`, keyed on a hash of every file the model reads plus the format
       version. An unchanged model is never cooked twice, and unused cooks are pruned.
     - Hello renders from cooked bundles within 13 levels of its source files, the difference being BC7.
     - *Planned:* the other BC7 modes for sharper colour, and BC6H for HDR textures.
   - **Pictures used on their own** (material maps, cookies, shader properties, lookup tables) are cooked too
     (`crates/voxear_render/src/cooked_picture.rs`): their import settings applied, then BC7 with CPU mips, or BC5
     for one the shipped content uses only as a normal map. The build learns each picture's use from the key that
     names it (`normal_map`, `metallic_map` and so on). One left uncompressed ships its RGBA8 pixels, and one
     shaped as a lookup table keeps them exactly. Their cooks are cached beside the models'.
   - Other assets (levels, scenes, prefabs, materials) ship as their source text, compressed as the build asks.

**Runtime** *(implemented: reading through a catalog, opening bundles on first use, batched async IO, GPU
decompression, background loading, unloading)*.
- Every read goes through an `AssetReader`: `FolderReader` in a project, and `BundleReader` for built content. Levels,
  scenes, prefabs, materials and models, with their buffers and textures, all load the same way from either.
  `Content::open_bundles` and `vox run --bundles` use bundles. Each bundle is logged when it opens, naming its
  roots.
- Loading a `Ref` resolves its bundle from the catalog, opens it, and returns the asset.
- **Async IO.** Readers take batches (`AssetReader::read_many`), and the IO layer (`voxear_assets::io`) works on a
  whole batch at once: **io_uring** on Linux, **IoRing** on Windows 11 (loaded at runtime, so Windows 10 still
  starts), and one read after another where neither is available. Background loads read in waves, one batch per
  step down the dependency graph: the level, then its scenes, then everything they use. On an SSD a wave then
  takes about as long as its slowest read.
- *Planned:* uploads on a queue of their own, so the main thread never waits for one.

**Unloading** *(implemented)*. It works like Unity's `UnloadUnusedAssets`, run for you after every level change.
- **What stays.** In use are:
  - the meshes and materials that live Objs draw with
  - prefabs that are kept, loaded or instantiated since the last sweep, or have a live instance, together with
    everything they draw with
  - models that an unfinished background load has uploaded
- **What goes.**
  - Everything else is freed. A model goes as a whole, with its meshes, materials and textures.
  - Texts read ahead and parsed prefab files are dropped, and bundles close.
  - Anything freed loads again by itself when used: a released prefab's id never goes stale.
- **GPU side.** The shared vertex and index buffers hand out ranges from a first-fit allocator whose freed ranges
  merge. Mesh and material table slots are reused, and textures are destroyed. Nothing is reclaimed until the
  frames still on the GPU are done with it.
- **The developer's controls** follow Unity's names:

  ```csharp
  enemyPrefab.Keep();           // survives level changes until Release, so it never loads again mid-game
  enemyPrefab.Release();        // freed at the next sweep once nothing uses it
  Content.UnloadUnused();       // sweep at the end of this frame, e.g. after a boss fight
  Log.Info(Content.Usage);      // models, prefabs and materials loaded; GPU meshes, materials, textures
  var load = Level.LoadAsync(next, activate: false);   // preload, change over when ready
  ```

  In Rust these are `Game::release_unused`, `Content::keep_prefab`, `Content::usage` and
  `Renderer::asset_usage`.
- In the editor the same API reads straight from the asset database, so no build is ever needed to press Play.
- Catalogs are **layered** *(implemented)*, so mods, patches and DLC add or override assets through the same path.
  See [Modding](#modding).
  - `Content::mount(dir)` puts built content on top. In the CLI, `vox run --bundles --overlay <dir or mod project>`
    does the same, and the flag can be repeated.
  - An overlay asset with the GUID of one below replaces it, so copying the original's `.meta` is how a mod
    overrides an asset. One with a new GUID is added, and path lookups find the topmost layer's asset.
  - Each layer reads from its own bundle folder. A content-only mod is just a project with `[content] include`
    patterns for what it replaces.
  - Mount before loading: what is already loaded stays as it was.

## Rendering

**Frame structure.**
- A **render graph** orders passes, places barriers automatically, aliases transient resources, and schedules async
  compute.
- **Several views in one frame** *(implemented)*: each view keeps what it needs from frame to frame (its camera's
  culling results, LOD fades, exposure and TAA history) and records its own passes into the same graph, its
  transient images named apart (`FrameGraph::set_prefix`). The first view clears around itself; the UI, previews
  and captures are drawn once, after every view.
- **Several outputs** *(implemented)*: besides the window or image it was made with (`OutputId::MAIN`), the
  renderer draws into windows added since (`add_window_output`), as the editor does for panels floated onto the
  desktop, or images for tests (`add_offscreen_output`, read back with `read_output_pixels`). Each output has its
  own UI (`set_output_ui`, and `set_output_game_ui` for the game's), and each view names the output it draws into
  (`set_view_output` for the first, `set_more_views_in` for the rest). All share the device, the graph and one
  submit, which presents every window drawn. The first view drawn into an output clears around itself, and an
  output no view drew into is cleared for its UI. The UI's quads live in a buffer of their own, every output's
  one after another, named by pointers of their own, so a UI draws whether or not any view did. Another window's
  swapchain takes the first's format, so the same pipelines draw into it (`Swapchain::new_matching`), and does
  not wait for the display: its image is acquired without waiting (`Frames::try_acquire`, with a semaphore of its
  own for each frame in flight), so a window the desktop is not showing never holds up the rest, and it presents
  with mailbox where it can.
- Resources are fully bindless through descriptor indexing and buffer device address.

**Geometry.** GPU-driven, with occlusion culling and LOD built in. See
[Geometry: GPU culling and LOD](#geometry-gpu-culling-and-lod) below.

### Geometry: GPU culling and LOD

Culling and LOD run entirely on the GPU, and are on for every mesh by default with no setup.

**Easy.** Import a mesh and it already has LODs, and it is already occlusion culled. The inspector shows the LOD
chain with live triangle counts, and a slider sets how aggressive LOD is per mesh or per quality tier.

**Advanced.** Hand-authored LOD meshes, per-LOD materials, custom screen-size thresholds, forced LODs for
cinematics, and per-camera culling overrides.

**GPU occlusion culling** is two-phase, using a hierarchical depth pyramid (Hi-Z):
1. **Phase 1.** Test every instance against the frustum and against *last frame's* depth pyramid. Draw the
   survivors, which are the objects that were visible last frame.
2. **Build the pyramid.** Rebuild the depth pyramid from that depth in one pass (single-pass downsampler).
3. **Phase 2.** Retest only the instances that failed phase 1 against the *new* pyramid, and draw any that are now
   visible. This catches disocclusion with no popping and no CPU readback.

The same tests then run per meshlet: frustum, backface cone and Hi-Z. Mesh shaders do this where available, and a
compute pass with indirect draws does it elsewhere. Shadow passes reuse the culling per cascade, so hidden casters
cost nothing. The CPU never touches visibility.

**LOD.**
- **Generated on import** with meshoptimizer. Simplification keeps UV seams, normals and silhouettes.
  Far LODs use more aggressive simplification. Every LOD is meshletised.
- **A model's own LODs** *(implemented)*: parts named `Rock_LOD0`, `Rock_LOD1` and on, as Unity names them,
  fold into one part, `Rock`, whose mesh takes the later LODs' vertices after its own, each LOD's triangles
  pointing into its share (`model_lods.rs`). Each is given the error a meshoptimizer simplification of the first
  to as many triangles would have, so the GPU picks it where it would pick a made LOD of that size. The import
  setting `own_lods` (on by default) chooses them over made ones; either way the later LODs are not parts.
- **Selected on the GPU by screen-space error**, not by distance. The same mesh picks the right LOD at any FOV or
  resolution.
- **Transitions are dithered cross-fades**, smoothed by TAA, so LOD changes do not pop. Each instance's LOD
  history is kept by its place in the frame's list of instances, so an Obj new to its place (`INSTANCE_NEW`, set
  when another Obj had the place last frame, as when a scene opens in place of another) starts afresh rather than
  fading in from the LOD another Obj was drawn at there.
- **The Inspector shows a mesh's chain** *(implemented)*: under its Mesh Renderer, a cell for each LOD with its
  triangles, the one the Scene view draws outlined, worked out on the CPU as the GPU picks (`lod::lod_at`, from
  the chain `Assets` keeps for each mesh). A click moves the Scene view to where that LOD is drawn.
- **Per-renderer LOD settings** *(implemented)*, as Unity 6's Mesh LOD: `MeshRenderer::lod_bias` scales the
  error the GPU allows it by two to that power, and `forced_lod` draws one LOD at every distance. With
  `lod_transitions`, the share of the screen's height each LOD shows down to, the GPU picks by how much of the
  screen the mesh covers from the main camera instead (`INSTANCE_LOD_SCREEN`, with the ends in
  `GpuInstance::lod_screen`), and the Inspector edits them on a bar as a LOD Group's, dragged from where the
  engine's own switching puts them for the Scene view. Each instance
  carries them to the cull shader (`GpuInstance::lod_bias`, and the forced LOD plus one in its flags above
  `INSTANCE_LOD_SHIFT`); shadows pick the same way. Scene files write `lod_bias` and `force_lod` on
  `[obj.renderer]`, and the Inspector edits them as LOD Bias and with Force on a LOD cell's menu.
- **LOD Groups** *(implemented)*: a `LodGroup` on an Obj shows one of its levels at a time, each a list of Objs
  as Unity's LOD lists renderers (`LodLevel::objs`), each Obj with its children, by the share of the screen's height its meshes cover or by its distance from the camera, as Unity's
  LOD Group does (`lod_group.rs`). Each view chooses for its own camera while extracting (`LodChoices`), and what a
  level not shown holds is neither drawn nor lit; its scripts run on. Files write it as `[obj.lod_group]`, levels
  naming Objs as other references do (`objs = [2, 5]`, with a lone `obj` read from older files), so prefabs map them into each instance (`lod_groups.rs` in the engine). Add
  Component makes levels of the children in Hierarchy order: those named as a model's LODs (`_LOD0`, read by
  `lod_of`, as model import does) by their number, the rest a level for each name, so a model's parts of one name
  share one. An Obj is in one level at a time (`LodGroup::put`), and Objs dragged from the Hierarchy onto a level
  of the bar, or the whole selection when the dragged one is in it, go in that level. The Inspector edits it as Unity's does, on the bar
  (`lod_inspector.rs`): a click selects a level, whose rows alone show, a drag on an edge moves where it ends, a
  right-click inserts or deletes one, and a camera mark shows where the Scene view sees the group.
- **HLOD:** distant groups of objects merge into one proxy mesh, generated per level.
- **Impostors:** octahedral impostors for foliage and far props, generated automatically.

**Toward virtualised geometry (Nanite-style).** Meshlets, the visibility buffer and two-phase Hi-Z culling are the
foundations Nanite is built on, so this is a later step rather than a rewrite:
1. Build a **cluster DAG** at import: group meshlets, simplify each group with its boundary locked, re-split, and
   repeat. meshoptimizer provides the partitioning and simplification steps.
2. **Select continuous LOD per cluster** at runtime by projected error. This removes discrete LOD switches and
   cracks.
3. **Stream cluster pages** from disk on demand through Content, so geometry memory scales with screen pixels.
4. **Rasterise tiny triangles in software** in a compute shader, using 64-bit atomics into the visibility buffer
   where the GPU supports them, with a hardware fallback.

**Lights.** All light types are clustered:
- directional (sun and moon)
- point and spot
- rectangle and disc area lights, using LTC
- tube lights
- sky light and emissive surfaces
- IES profiles and light cookies

**Shadows.** Cascaded shadow maps for the sun first, then virtual shadow maps. Soft shadows use PCSS, and contact
shadows cover small detail. Local lights that ask for shadows share an atlas of their own: a spot light one view, a
point, area or tube light a cube's six, the nearest eight lights a frame.

**Atmosphere.** A physically based sky (Hillaire 2020) and **volumetric fog**. The fog is a froxel volume lit by
every light type and by GI, with temporal reprojection, local fog volumes and height fog.

**Post-processing.** Built in and on by default as the Voxear Look. See
[Post-processing and the Voxear Look](#post-processing-and-the-voxear-look) below.

**Global illumination** comes in two forms:

- **Baked:**
  - A GPU path-traced lightmapper, using hardware ray tracing when available and a compute BVH otherwise.
  - Light probes (L2 SH, placed automatically) and reflection probes.
  - Bakes are progressive in the editor viewport.
- **Real-time voxel GI**, all in Rust and Slang on the GPU:
  1. **Voxelise** the scene into camera-centred **clipmap cascades**, for example 4–6 levels of sparse 64³ bricks.
     Brick storage is sparse, so only surface bricks exist. Cascades scroll toroidally, so moving the camera only
     re-voxelises the newly exposed slabs. Dynamic objects re-voxelise within a per-frame budget. Each voxel stores
     albedo, normal, emission and opacity.
  2. **Inject** light: direct light from the shadow maps and clustered lights, plus emissive surfaces. Last frame's
     GI is fed back in for infinite bounces at no extra cost.
  3. **Gather.**
     - Screen-space probes are placed sparsely (one per 16×16 pixels) and adaptively.
     - Each probe traces short screen rays first, then continues through the voxel field with cone or ray marching.
     - A world-space radiance cache handles distant lighting.
     - Results are denoised temporally and spatially at half resolution, then upsampled with a bilateral filter.
     - Hardware ray tracing replaces voxel marching when present.
  4. **Specular:** voxel cone tracing for rough surfaces, and screen-space reflections with a voxel fallback for
     glossy ones.
  5. Quality tiers scale cascade count, brick resolution and probe density, from integrated GPUs up to high end.

**Lighting looks** *(planned, M5)*. A scene picks a look, a preset for its sky, sun, shadows, fog, reflections and
post-processing together, as `[post]` picks one for post-processing today, and changes any part of it.
**Soft Overcast** is the default: the sky as the main light, a weaker warm sun with soft shadows, haze and sun
shafts, exposure set for interiors, and a soft filmic finish. **Storybook** is stylised: a warm sun through a
canopy into a cool teal forest, tinted shadows, glowing foliage and rich colour. The roadmap's M5 has each one's
settings and what it needs.

### Previews *(implemented)*

A preview is an object drawn on its own into a small image: the editor's Project panel, reference fields, object
picker and Inspector show them, and games ask for them for inventory icons, shop items and the like.

- **What shows** (`voxear_engine::preview_parts`): a material on a sphere; a prefab as all its Objs' meshes where
  they sit in it; a model as all its nodes; a mesh such as `builtin:cube` on its own. A scene or a sound has none.
- **How it looks** (`PreviewOptions`): its size (up to 512 pixels), the background (any sRGB colour, or clear),
  the object's rotation, an offset across the frame, zoom, the field of view (small looks flat, like an icon), the
  key light's direction, colour and strength, the sky's ambient, and exposure. The defaults suit an icon: a
  three-quarter view from a little above, framed to fit its bounding sphere, on a clear background.
- **Drawing** (`voxear_render::previews`, `shaders/preview.slang`): previews are drawn a few a frame, in the frame's
  own graph before the UI, each into its own image at two to four times its size with its own depth, lit as in a
  studio (the key light, a fill from the camera's side, the sky's ambient), then averaged down, tonemapped and laid
  over its background into a cell of one atlas texture, sRGB and premultiplied. The atlas starts at 1024 pixels
  square, splits into power-of-two cells as a quadtree, grows once to 2048 when it fills, and after that gives the
  cell of the preview looked at longest ago to a new one; a preview that lost its cell is drawn again when next
  looked at. Its meshes and materials need only live until it is drawn.
- **Using one**: `Renderer::add_preview(parts, options)` asks for one, `preview(id)` says where it is once drawn
  (the atlas's texture and the preview's rectangle in it, for a UI `Image`), `set_preview` draws it again turned or
  changed, and `free_preview` lets it go. `read_preview` copies its pixels back from the GPU, which
  `preview_pixels` hands over a few frames later, straight alpha, and `PreviewPixels::save_png` writes them.
- **Easy and advanced**: `AssetPreviews::image(content, renderer, "Prefabs/Lamp.prefab", 64)` keeps one per asset
  and size, as the editor uses them; `preview_parts` and `add_preview` take any options.
- **From C#**: `Preview.Of(prefab)`, `Preview.Of("Materials/Gold.material")` or `Preview.Of(someRef)`, with an
  optional `PreviewOptions { Size, Background, Rotation, Offset, Zoom, FieldOfView, LightFrom, LightColor,
  LightIntensity, Ambient, Exposure }`. `IsReady` says when it is drawn, `Save("icon.png")` writes it a few frames
  later, `Texture` and `Region` are for the C# UI to come, and `Release` lets it go (`ScriptPreviews` in the
  engine).

### Post-processing and the Voxear Look *(implemented; upscalers, LUTs, volumes and the C# API planned)*

Every project starts with a finished look, so nothing has to be set up for a scene to look good. Each effect is a
plain setting, nothing is hidden behind a profile asset, and the shaders are short and commented.

**Easy.** Pick a preset and change what you like:

```toml
# In any .scene file
[post]
preset = "voxear"          # or "neutral", "cinematic"

[post.bloom]
intensity = 0.06
```

| Preset | What it is for |
| --- | --- |
| `voxear` (default) | TAA with light sharpening, soft bloom, eye adaptation that keeps nights dark (+0.5 EV compensation), AgX with a little contrast and saturation, a soft vignette and a touch of height fog. |
| `neutral` | Only what keeps the image correct: TAA, AgX with no look, fixed exposure. A clean base to build a look on. |
| `cinematic` | ACES, stronger bloom and vignette, motion blur, a little grain and lens fringing. |

A section only lists what differs from its preset, and unknown keys are errors, so typos do not pass silently. Like
the sky, the last scene loaded with a `[post]` section sets post-processing for the level. Saving a scene writes
the section back as it was written, preset and overrides, not every resolved value.

**Advanced.** Every setting, with its Voxear Look default:

| Group | Settings |
| --- | --- |
| `anti_aliasing` | `mode` (`taa`, `fxaa`, `off`), `sharpness` 0.25, `taa_history` 0.97, `taa_jitter` 1.0, `specular_smoothing` 0.5 (specular anti-aliasing, 0 off) |
| `bloom` | `enabled`, `intensity` 0.04, `scatter` 0.7 |
| `exposure` | `auto`, `compensation` +0.5, `min_ev` −3, `max_ev` +1, `speed_up` 2, `speed_down` 1 (EV per second) |
| `tonemap` | `mapper` (`agx`, `aces`, `neutral` for Khronos PBR Neutral, `reinhard`, `none`), `contrast` 1.1, `saturation` 1.15 |
| `grading` | `temperature`, `tint`, `contrast`, `saturation`, and per-channel `lift`, `gamma` and `gain` |
| `vignette` | `intensity` 0.2, `smoothness`, `roundness` |
| `fog` | `enabled`, `density` 0.004, `height_falloff`, `base_height`, `color` (the sky's horizon when unset), `sun_glow` |
| `depth_of_field` | off; `focus_distance`, `aperture` (f-stop), `max_blur` (pixels at 1080p) |
| `motion_blur` | off; `intensity` (fraction of a frame's motion) |
| `chromatic_aberration`, `grain` | off; `grain.size` |

In Rust the same settings are `PostProcess`, a world resource. `ViewOptions::post_override` replaces it for one
view, and `vox render --post <preset>` uses that to compare looks. `Renderer::reset_history` drops TAA and motion
history on camera cuts.

**The passes,** each skipped when its effect is off:
1. **Velocity.** Opaque and sky passes also write screen motion. Each instance keeps last frame's model matrix, so
   moving and spinning objects get correct vectors, and new ones count as still.
2. **Fog:** exponential height fog integrated along each view ray, glowing towards the sun. The froxel volumetric
   fog in [Atmosphere](#rendering) replaces it in M5.
3. **TAA.** The camera is jittered along an eight-frame Halton sequence. Each pixel reprojects last frame's result
   with a nine-tap Catmull-Rom filter, using the nearest surface's motion at edges. History is clipped to the
   neighbourhood's colour box in YCoCg, and blended in a tonemapped space so highlights do not flicker. Fast
   motion keeps less history. History resets on the first frame, on resize and when TAA is switched on.
4. **Depth of field:** a 48-tap golden-angle gather whose weights keep sharp foreground edges from bleeding.
5. **Motion blur:** 12 taps along each pixel's motion, capped at 40 pixels.
6. **Bloom:** six levels down with the 13-tap filter from Call of Duty: Advanced Warfare (Karis-averaged on the
   first level, so single bright pixels cannot flicker), then back up with a tent filter.
7. **Exposure:** a compute pass meters the bloom level an eighth of the screen across (centre-weighted log
   average). It adapts at the configured speeds and keeps its state on the GPU between frames. `vox render` prints
   the adapted EV and the metered luminance.
8. **Composite:** chromatic aberration, bloom, exposure, white balance, contrast and saturation, the tonemapper,
   lift/gamma/gain, vignette and grain.
9. **Finish:** FXAA or contrast-adaptive sharpening (AMD CAS), then a one-code-value dither in sRGB space against
   banding. Without either, the composite writes straight to the output and dithers there.

**Cost** at 1280×720, measured with the GPU timers:
- The Voxear Look adds about 0.25 ms. TAA takes 0.06 ms, composite 0.04 ms and finish 0.03 ms.
- Depth of field, when on, takes about 0.28 ms.

**Planned:**
- Upscalers: FSR built in, DLSS and XeSS optional. They take TAA's place in the chain.
- Colour grading LUTs, exportable to and from image editors.
- Local post volumes that blend by distance (Unity's local Volumes), and per-camera overrides.
- An inspector with live previews.
- A C# API: `Post.Current.Bloom.Intensity = 0.1f` for the easy path, and the full struct for the advanced one.

## Materials and Shader Graph

**Shaders.** Shaders are Slang modules. A material is a Definition: a shader plus parameter values. Material
variants inherit from a parent and override only what differs.

**Easy.**
- A material browser with live-rendered presets: PBR lit, unlit, foliage, glass, water, skin, hair, decal, UI, and
  particle lit/unlit.
- Drag a texture onto an Obj and a material is made for it.

**Advanced.**
- **Shader Graph:** a node graph that compiles to Slang.
- Every node has an inline preview, and sub-graphs and custom Slang nodes are supported.
- Hand-written Slang shaders can implement the same surface interface, so graph and code materials mix freely.

**Compilation.** Permutations are tracked and compiled in parallel in the background. A pipeline cache persists
across runs, and the build pipeline pre-compiles the permutations used by shipped content.

## UI (Voxear UI) *(implemented: tree, layout incl. grid, text, drawing, markup, style sheets, transitions, pointer and keyboard, scroll/list/tree views, input controls, split views, tabs, dropdowns, docking, default theme, the game's UI in C#, UI panels in the world; data binding planned)*

**Recommendation from research.** Build our own retained-mode UI on the Linebender and Servo crates, rendered by our
own Vulkan code.

Nothing off the shelf meets all of these requirements together:
- one system for the editor and games
- screen space and world space
- rendering through our ash renderer
- markup with stylesheets
- C# binding
- shipping licence

In particular:
- Slint, iced, Xilem, Vello and Bevy UI are tied to wgpu, Skia or GL. Slint's royalty-free licence also forbids
  exposing its APIs, which an engine does.
- RmlUi has no shaping or BiDi.
- Noesis is paid per project.
- A web-based editor cannot provide the in-game UI.

The building blocks are already the Rust standard, and all are MIT, Apache or MPL:
- **taffy** for flexbox, grid and block layout
- **parley + fontique + harfrust + ICU4X** for shaping, BiDi, line breaking and font fallback (CJK included)
- **swash** for rasterising hinted glyphs. It sits on the same font data as parley, and skrifa (Chrome's font
  backend on Linux) remains the option if its hinting ever falls short.
- **cssparser + selectors** for the stylesheet engine

What we write is the cascade, tokens, transitions, widgets, the renderer, and the C# binding layer.

**What exists now** (`voxear_ui`, and the UI pass in `voxear_render`):
- A retained tree of elements, laid out by **taffy**, a layout engine implementing CSS flexbox, grid and block
  layout (Bevy's UI and Dioxus use it too). Flexbox and grid are both exposed: `display: grid` with
  `grid-template-columns: 120px 1fr`, `repeat()`, `minmax()`, `grid-column: 1 / span 2` and the rest, which taffy
  parses itself, so the syntax is exactly CSS's. The inspector's label and field rows are a two-column grid. Text joins the layout through a measure callback: parley shapes it at the width offered, so
  wrapping text gets the right height. taffy caches layouts, so only what changed is laid out again.
- Layout is in logical pixels; text is shaped at the display scale, so hinting and line breaks match the pixels
  it lands on. Every element is positioned (`position: relative`), so `position: absolute` places a child in its
  parent, as in Unity's UI Toolkit, not in the nearest positioned ancestor as CSS's `static` default would.
- Text is shaped by parley from the system's fonts (fontique), Inter first for the UI and JetBrains Mono for
  numbers, with fallbacks. swash rasterises each glyph hinted at its physical size into a 1024² coverage atlas,
  packed in shelves, and glyphs are placed on whole pixels.
- `Ui::frame` lays out what changed and returns quads: SDF rounded rectangles with borders, and glyphs. Nothing in
  `voxear_ui` touches the GPU, so it is tested without one.
- The renderer draws every quad in one draw call after post-processing, from a per-frame buffer. Colours are linear
  and premultiplied and the output is sRGB, so blending happens in linear light. Text coverage is leaned back
  towards how the face looks in a design tool, since linear blending makes light text on dark look heavier.
- The atlas uploads only when new glyphs appear: every frame carries it shared and versioned, so the renderer
  uploads a new version even if frames were skipped. When it fills it starts again and redraws.

```rust
let mut ui = Ui::new();
let panel = ui.panel(ui.root(), Style::column().at(12.0, 12.0).padding(12.0).radius(10.0).background(Color::hex(0x111216)));
let row = ui.panel(panel, Style::row().spread());
ui.label(row, "GPU", TextStyle::default().color(Color::hex(0x8b8c96)));
let gpu = ui.label(row, "–", TextStyle::default().mono());
ui.set_text(gpu, "0.43 ms"); // costs nothing when unchanged, so call it every frame
renderer.set_ui(&ui.frame(width, height, scale))?;
```

The first thing built with it is the stats overlay (see [Debugging and profiling tools](#debugging-and-profiling-tools)).

**Markup and style sheets** *(implemented)*. A `.vui` document says what the elements are, and `.vss` sheets say
how they look, as UXML and USS do in Unity:

```xml
<vui>
    <Style src="StatsOverlay.vss" />
    <Panel name="stats" class="stats">
        <Panel name="header" class="header">
            <Label class="title">Stats</Label>
        </Panel>
        <Panel class="row"><Label>GPU</Label><Label name="gpu" class="value">–</Label></Panel>
    </Panel>
</vui>
```

```css
:root { --surface: #111216eb; --accent: oklch(70% 0.14 275); }
.stats { position: absolute; left: 12px; top: 12px; padding: 12px; gap: 6px; border-radius: 10px;
         background-color: var(--surface); border: 1px solid #2a2b30; font-size: 12px; }
.header:hover .title { color: var(--accent); }
.collapsed .body { display: none; }
```

- **Elements:** `Panel`, `Label` and `Button`, each with `name`, `class` and `style` (declarations that win over
  sheets). Text is the element's content. `<Style src>` attaches a sheet to the element it is in.
- **Selectors:** USS's set, which is types, `.class`, `#name`, `*`, the states `:hover`, `:active`, `:focus`,
  `:focus-visible`, `:disabled` and `:checked`, and descendant and `>` combinators. `:root` is the element a sheet is attached to,
  so each document keeps its theme tokens at its own top.
- **The cascade:** the style given in code first, then matching rules from least to most specific (a nearer
  sheet, then a later rule, wins a tie), then inline declarations. Text styles and custom properties
  (`--name`, read with `var()`) are inherited, as in CSS. Only what changed is styled again: a class, state or
  style changed on an element restyles that element, and what is inside it only when a selector asks that of an
  ancestor (`.dark .label`) or what it inherits changed. Each sheet files its rules by the name, class or type
  they style, so an element is checked against those that could match it, not every rule. With the editor's
  1,300 elements and 437 rules, an idle frame went from 8 ms of styling to none *(implemented)*.
- **Properties:** CSS's names for flexbox layout, sizes, margins, padding, gaps and positions, plus
  `background-color`, `border`, `border-radius`, `opacity`, `pointer-events`, `color`, `font-size`,
  `font-weight` and `font-family`. Lengths are `px` or `%`. Colours are hex, `rgb()`, `rgba()`, `oklch()` or a
  few names.
- **Mistakes** are reported once each, with the file, line and what was wrong, such as
  `Stats.vss:12: unknown property 'colour'` or `Test.vui:2:3: unknown element <Lable>`. A broken rule is
  skipped, as browsers do, and the rest of the sheet still applies.
- **The pointer** sets `:hover` on the element under it and everything it is in, and `:active` while pressed.
  A press and release on the same element is a click, which code reads with `ui.clicked(node)`, as the overlay
  does to fold itself away. When what was pressed is built again before the release, as a panel rebuilt by a
  value typed a moment before, the release clicks what is under the pointer then, if the pointer has hardly
  moved (5 pixels). A double click is two clicks within 0.4 seconds and 5 pixels, as desktops tell them, so it
  holds when the first click rebuilt what it landed on. Disabled elements cannot be clicked, and
  `pointer-events: none` lets the pointer through. A document's container lets it through too, so UI over a game
  takes only the clicks it is under.
- **Overflow.** `overflow: hidden` cuts off what an element holds at its edge, its own text included, so a
  label in a table's cell keeps a long line within its column instead of running over the next.
- Code finds elements by name with `ui.find(document, "gpu")`, and changes them with `set_text`, `add_class`,
  `set_enabled` and the like.

**Controls** *(implemented: scroll, list and tree views)*:
- **ScrollView**, or any element with `overflow: scroll`: children are clipped to its padding box and moved by
  its scroll offset. The wheel scrolls the innermost one under the pointer that can still move that way, and
  a thin scrollbar floats over the right edge, as in Linear, to drag or click. Children of a scrolling element
  keep their size rather than shrinking to fit, as in Unity's ScrollView. Clipping happens in the vertex
  shader, which trims each quad to its clip rectangle, and whatever lies wholly outside is never drawn.
- **ListView** keeps elements only for the rows on screen, so 100,000 items cost what 20 do. As in Unity, rows
  are built once by `make` and filled by `bind` whenever they show a different item, and are reused as the list
  scrolls. Rows match `:selected`; a click selects, Ctrl adds or removes, and Shift extends. A click on a row also
  gives the list focus, and then the arrow keys move the selection, Shift extending it, with Home, End and the page
  keys; in a tree, Right opens an item or goes into it, and Left closes it or goes to its parent, as desktops'
  trees do.

  ```rust
  let log = ui.list_view(panel, 18.0, lines.len(), ItemFns {
      make: |ui, row| { ui.label(row, "", TextStyle::default()); },
      bind: move |ui, row, index| ui.set_text(ui.children(row)[0], &lines[index]),
  });
  ```

  Markup can declare one (`<ListView name="log" item-height="18" />`), and code then gives it items with
  `bind_list`, as Unity does with `itemsSource`.
- **TreeView** is a list of the open items of a tree, by `u64` id: rows indent by depth, a chevron opens and
  closes them, and selection follows items rather than rows. The Hierarchy and the Project's folders are tree
  views. Every row ends in a `row-bar`, which the default theme shows as an accent bar down a selected row.
- **The editor is built from these.** Every long list in the editor (Hierarchy, Project, Console, inspector
  arrays) is a virtualized list or tree view, never one element per item.
- **In a game's scripts** *(implemented)*, `ListView<T>` and `TreeView<T>` are these views, made in code as the
  other elements are, with Unity's `makeItem` and `bindItem` as `MakeRow` and `BindRow` (a label of the item's
  text without them). A list sends its count, and a tree its items as ids with their parents' (`ui_items`, API
  31); the engine's `GameUi` queues what the views make and bind, and after the scripts' updates it lays the UI
  out (`Ui::prepare`) and has the scripts make and fill those rows in one JSON call (`fill_rows`), so a row shows
  its item in the very frame it scrolls on. The selection is the element's value: a list's index, or a tree's
  item id, which the scripts give each item and keep across a `Refresh`.

**Input controls** *(implemented)*. Each is made of ordinary elements with classes, so a sheet can restyle
every part:
- **TextField**: one line, or several with `multiline`. Editing is parley's own: the caret and word movement,
  selection by drag, double click (a word) and triple click (a line), Ctrl+A/C/X/V, Ctrl+Backspace and Delete
  by word, a placeholder while empty, and input-method composition with `ime_area` for the candidate list. A
  long line scrolls to keep the caret in view, and the text is clipped to the field. A multiline field scrolls
  down to keep the caret in view too, and with the wheel, showing a thumb as a scroll view does; Page Up and Page
  Down move by a field's height, and Ctrl+Home and Ctrl+End go to the start and end. **Undo**: Ctrl+Z undoes a
  word typed at a time, or each other change, and Ctrl+Shift+Z or Ctrl+Y redoes; text set from code starts the
  history again.
- **Toggle** (a switch: a 28 by 16 pill with a sliding knob, or a ticked box with the `checkbox` class),
  **Slider** (dragged, or moved by the arrow keys, with an optional step, and its value shown in mono to as many
  decimals as the step has) and **Foldout** (a header with a chevron that opens and closes its content; markup
  children go inside it).
- **Segmented**: a row of segments, each text, an icon or both, exactly one on, for modes such as the transform
  tool or the pivot (`<Segmented value="1"><Segment icon="move" />...</Segmented>`).
- **Focus and the keyboard:** clicking a control focuses it, Tab and Shift+Tab walk the controls in tree order
  (skipping hidden and disabled ones), and the focused one matches `:focus`. It matches `:focus-visible` too when
  Tab reached it, or when it is a text field, as in browsers; the default theme draws focus rings on that, so a
  button pressed with the pointer keeps no ring as if marking what was pressed last. Enter and Space press a focused
  button or toggle, and Escape lets go. While a text field has focus, keys belong to it (`wants_keyboard`),
  so typing a W never moves the camera, and in the editor Ctrl+Z, Ctrl+Y and Ctrl+D are the field's, not the
  scene's.
- **Events**, read during the frame like clicks: `ui.changed(node)` when the user changed a value,
  `ui.submitted(field)` for Enter, and `ui.double_clicked(node)` for a second click in quick succession. Setting
  a value from code is not a change.
- **The clipboard** is a trait the window provides (`voxear_engine::ui_input::SystemClipboard` wraps the
  system's, through arboard), so `voxear_ui` holds no platform code and tests use an in-memory one.
- `voxear_engine::ui_input::handle` turns every winit event into the UI's calls, and returns whether the UI used
  it, so the game leaves it alone.

**Split views, tabs and dropdowns** *(implemented)*, which docking and the editor's panels are made from:
- **SplitView**: two panes either side of a divider the pointer drags. The first pane keeps its size and the
  second takes the rest, each kept above a minimum, as in Unity's TwoPaneSplitView. A negative size keeps the
  second pane's size instead (a panel on the right, such as the Inspector), and `split_view_fraction` (or a
  percentage `size` in markup) keeps the first pane's share as the window resizes. While a divider is hovered or
  dragged, the default theme lights a short handle across its middle (`split-handle`), drawn over everything so
  no panel's edge hides it.
- **Tabs**: a row of tab buttons over pages, showing the selected one (`:checked` on its tab).
- **Dropdown**: opens its options in a **popup layer** that is always drawn above everything and takes the
  dropdown's text style. The list is at least as wide as the field and as wide as its longest option needs, unless
  `set_dropdown_popup_width` fixes it, and opens below the field, or above it when there is no room below, kept
  inside the window either way (as context menus are). A choice, Escape or a press anywhere else closes it, and
  that press goes no further. The arrow keys change the choice while it is focused.

**Docking** *(implemented)*, the editor's layout: a `DockSpace` is a tree of split panes whose leaves are
tabbed areas of panels.
- Dragging a panel's tab onto another area's middle adds it there as a tab; onto an edge (the outer quarter),
  it splits that area. A ghost of the tab follows the pointer and a preview shows where it will land. Areas
  left empty fold away, and a press that never moves stays a click on the tab.
- A panel keeps its own elements wherever it goes, so a half-typed field or a scrolled list survives a move.
- The layout reads and writes as text, for the editor's layouts and saving between sessions:
  `row(240, [hierarchy, project], column(420, [scene, game*], [console]))`, where `row` and `column` give the
  first part's size in pixels (or the second's, negative, or the first's share, as `57%`) and `*` marks the
  showing tab. Divider moves and tab choices are part of it. Panels a layout leaves out join its first area, or,
  with `set_dock_layout_closing_rest`, are closed.
- **Closing and opening panels.** `close_dock_panel` takes a panel's tab away, folding an area left empty, and its
  elements stay alive out of sight; any panel can close, the last one too. `open_dock_panel` brings it back as a
  tab beside a panel it shared an area with, or in the first area, and `open_dock_panel_beside` beside another.
  Each area and floating window has an options button at its tab bar's right end, which `dock_menu_clicked`
  reports with the panel showing there, for the window's menu. `dock_panels` lists every panel with whether it is open, and
  `dock_tab_context_clicked` says whose tab was context-clicked, for a menu; a context click on a tab bar's empty
  part is one on its tab showing.
- **Where a dragged tab lands.** Over a tab bar it joins that area or floating window as a tab; near an area's
  edge (its outer quarter) it splits the area; in the middle of another area or window there is nowhere to dock,
  so it goes out on the desktop in a window of its own, as Unity's floating windows do. In the middle of where it
  came from, it stays.
- **Floating windows** *(implemented)*. A panel floats in a window of its own over the dock with
  `float_dock_panel`, which the editor no longer offers (its Float goes out on the desktop), though layouts that
  hold such windows still open them, with Float to take them out and Dock to put them back. Dragging the
  window's bar moves it, the grip in its corner resizes it, and both stay inside the dock. Its tabs dock again as
  any tab does (or with Dock on the menu, `dock_floating_panel`); other tabs dropped on its tab bar join it. A
  window is the dock's last child, so it is drawn over the areas yet under menus opened from it. Floating windows
  follow the tree in the layout's text, each with its place and size:
  `row(240, [hierarchy], [scene]) float(600, 80, 420, 320, [inspector])`, so they are kept between sessions too.
- **Desktop windows** *(implemented)*. A floating window can be a window of the desktop's own instead, out of the
  main window altogether: `float_dock_panel_in_window` (Float on a tab's menu in the editor), which takes a
  floating window out whole or else the panel alone at its area's size; a tab let go outside every area or past
  the window's edge it was dragged in, or a floating window's bar let go past the main window's edge; or a tab dragged out of a desktop
  window that holds others. `dock_windows` lists them (`DockWindow`: its `DesktopWindow` id, panels, the title
  showing, its size and, where the desktop says, its place), for whoever owns the windows to open one for each,
  and `set_dock_window_size` and `set_dock_window_place` tell the dock what the desktop did to it. The desktop
  moves and resizes it, so it has no grip, and a press on its bar is handed over (`take_window_move`) for the
  desktop to move it while the button is down. Its tabs dragged onto the main window dock there, and tabs dragged
  onto its bar join it, as any floating window's do. In the layout's text it has its size, and its place when
  the desktop said: `window(420, 320, [inspector])` or `window(420, 320, 1600, 90, [inspector])`.

  Underneath, an element shown in a desktop window stays in the one tree, styled and laid out with everything
  else, so nothing in it is rebuilt as it moves out and back. It is placed at the screen's top left at its
  window's size (moved there after a layout, as only a layout says where its parent is), so its coordinates are
  its window's own: `rect` of anything in it, and the pointer in that window, need no converting. `frame` leaves
  it out and `window_frame` draws it, with its own glyph atlas upload. `set_pointer_window` says which window the
  pointer is in before its moves and presses there; it then finds only what that window shows. Menus and
  dropdowns opened there open in a layer of that window's (`window_of` says which window an element is in), a
  dragged tab's ghost and preview follow the pointer from window to window, and `show_over_window` moves the
  caller's own overlays, such as the editor's object picker, over the window of the field that opened them, and
  back. Everything in the UI is drawn at the main window's display scale, so a desktop window on a screen of
  another scale shows it at the first screen's size.

  ```xml
  <DockSpace layout="row(240, [hierarchy], [scene, inspector])">
      <DockPanel id="hierarchy" title="Hierarchy">...</DockPanel>
      <DockPanel id="scene" title="Scene" />
      <DockPanel id="inspector" title="Inspector">...</DockPanel>
  </DockSpace>
  ```

**Transitions** *(implemented)*: `transition: background-color 120ms ease-out, opacity 0.2s` eases colours and
opacity when the cascade changes them, as hover, focus and selection do. Easing follows CSS's curves (`linear`,
`ease`, `ease-in`, `ease-out`, `ease-in-out`), colours blend in linear premultiplied space, and nothing animates
the first time an element is styled. Transitions run on the UI's clock, which `ui.advance(seconds)` moves each
frame; until it is first called, styles change at once, so a caller without a clock never sees a colour stuck
mid-fade.

**Keyframe animations** *(implemented)*: a sheet names keyframes as CSS does, and `animation` plays them:

```css
@keyframes busy-pulse { from { opacity: 1; } to { opacity: 0.25; } }
.status-dot.busy { animation: busy-pulse 700ms ease-in-out infinite alternate; }
```

- Stops are `from`, `to` or percentages, and several may share a block (`0%, 100% { ... }`). Colours and opacity
  animate, as they transition, and a stop's values are read as the element's own are, `var()` included. A stop
  left out at either end shows the element's own value.
- `animation: <name> <time> [easing] [delay] [<count> | infinite] [alternate]`: the easing applies between each two
  stops, as CSS's does; a count plays it that many times and then the element shows its own value again.
- An animation goes on from when it began while the element is styled again, and starts over only when the
  animation itself changes. `ui.is_animating()` is true while one plays. The editor's status dot breathes this way
  while the scripts build.

**Backdrop blur** *(implemented)*: `backdrop-filter: blur(16px)` shows the 3D view under an element blurred, with its
background over it, as frosted glass, inside its rounded corners. When a frame's UI has such an element, the
renderer keeps the finished view as an image of its own (the path FXAA takes anyway), halves it three times with the
13-tap filter bloom goes down with, which blurs as it shrinks, and the UI pass samples the smallest, four times
around each point so its texels never show (`post_passes::blur_for_backdrop`, `ui.slang`). It blurs the view, not
other UI, which is what overlays over the view sit on; a frame with none pays nothing.

**Icons and shapes** *(implemented)*:
- **Icons** are Voxear's own line icons (`voxear_ui::ICONS`: chevrons, play controls, tools, object and asset
  kinds, the Voxear mark), drawn on a 24-unit grid as SVG path data. Each is rasterised by swash's zeno into the
  glyph atlas at its exact pixel size, its lines 1.5 physical pixels wide at any size, so it is as crisp as text,
  and drawn in its element's text colour. `<Icon icon="move" />`, `ui.icon(parent, "move")` or
  `ui.icon_button(parent, "plus")`; a Button in markup may hold an icon and a label.
- **Images** show part of a texture the renderer holds, such as a preview: `<Image />` or `ui.image(parent)`,
  then `ui.set_image(node, Some(ImageRef { texture, uv }))`. The UI holds no textures itself; `texture` is the
  renderer's bindless index, `uv` the part of it, and `Renderer::add_texture` uploads a picture for one. An image
  fills its box, its corners round with `border-radius`, and the pointer passes through it; drawn smaller than
  its texture, it samples the mip that suits its size. `aspect-ratio` (`16 / 9`) keeps an image's shape as its
  width changes. Text that does not wrap still takes its `text-align`.
- **`box-shadow`** (`0 16px 40px rgba(0,0,0,.4)`, with an optional spread) draws a Gaussian-blurred copy of the
  box under it, cut out of the box as CSS's is. A spread with no blur makes a crisp ring, and a 1px offset a
  hairline under a header.
- **`border-style: dashed`** (or `dashed` in `border`) dashes a border, for drop targets such as Add Component.
- **`line-height`** sets the height of each line of text, so 12px text sits on a 16px line as in the design.
- **`z-index`** above 0 draws an element and its children after everything else, higher ones last, as for the
  split handles. The pointer still finds elements in tree order.
- **Shapes** (`ui.set_shapes`): lines with round ends and circles drawn on any element, for gizmos and graphs.
- **See-through elements** (`ui.set_see_through`): an element that is a window onto what the renderer drew under
  the UI, such as a 3D view. What the UI drew before it is cut away inside it, a quad partly inside drawn as the
  parts around it, so what lies under a floating window does not show through the view in it.
- **Gradients** (`ui.set_gradient`): an element filled from one colour at the left to another at the right, and,
  if asked, down to a third at the bottom, rounded as its corners are. The colours blend in sRGB, as people see
  them, so a hue bar or a colour picker's square looks even.

**The colour picker** *(implemented)*: `ColorPicker`, a group of ordinary elements built in code and asked each
frame, as design tools and Unity have it.
- A square of saturation (across) and brightness (down) for the hue picked on the bar under it, each with a mark
  that follows a drag, which holds at the edges when the pointer goes past them.
- The colour before and now side by side, its hex code, and its red, green and blue from 0 to 1 to type. A field
  being typed in is left alone while the rest follow.
- **HDR colours** add an Intensity in stops, a slider and a field from -10 to 10, which brightens the colour past
  white in linear light. `PickedColor::from_linear` splits an HDR colour into a colour and its intensity (the
  brightest channel above 1 becomes the stops), and `to_linear` puts them back, as Unity's HDR picker does.
- `update` returns the colour when it changed that frame; the default theme styles it (`color-picker`).

**Menus and dragging** *(implemented)*:
- **Context clicks**: the secondary button's press and release on one element (`ui.pointer_secondary`,
  `ui.context_clicked`). On a list or tree row that is not selected, it selects the row first, as file managers do.
- **Menus** (`ui.open_menu(owner, at, &[MenuItem])`): a `menu-popup` of items at a point, in the popup layer, moved
  back inside the screen once laid out. Items can be checked (with a tick), disabled, carry a shortcut shown at the
  right, or be separators. A choice comes back through `ui.menu_choice(owner)`; Escape or a press elsewhere closes it.
- **Dragging** (`drag`): a press that moves four pixels picks up what was pressed, the innermost of a row of a list
  whose rows drag (`ui.set_rows_draggable`) and an element made draggable (`ui.set_draggable`), and a `drag-ghost`
  with its text follows the pointer. An element that takes drops (`ui.accept_drops`) is marked `drop-hover` while
  it is under the pointer, and letting go on it is a drop onto it (`ui.dropped_onto`, with a `DragSource` saying
  what was dragged). Letting go elsewhere in a row's own list is a drop in the list, the row under the pointer
  marked `drop-target` (`ui.dropped`, or `ui.dropped_item` for a tree). A list whose rows take drops
  (`ui.accept_row_drops`) also takes what is dragged from elsewhere onto its rows, marked the same way before, onto
  or after the row under the pointer (`ui.dropped_on_item`), while its own rows still land in it as rows. Either way
  the release is no click.
- **Rich text** (`RichText`, `ui.rich_label`): a label whose words are bold, italic, code, struck out or
  highlighted, with links that `ui.clicked_link` reports. Each span shapes as its own run, so its colour, ground
  and lines are drawn with it; `link-color`, `code-background` and `mark-color` come from the text style, and
  italics and bold are slanted and thickened when a font has no such face. `white-space: nowrap` keeps a line
  whole, as list rows do, and `text-align` still places it in its box (at the start when it does not fit), so a
  list's columns of numbers line up on the right.
- **Icons in tabs and foldouts:** a `DockPanel`'s `icon` (or `add_tab_with_icon`) shows before its title, and
  `set_foldout_icon` before a foldout's; `foldout_header` takes buttons or a switch after the title, which work
  without opening or closing it.

**The default theme** (`crates/voxear_ui/theme/Default.vss`) is on every `Ui`'s root: the editor's design
(see the editor's design below), with its tokens as `--vx-*` variables, for buttons, fields, switches and
checkboxes, sliders, segmented controls, foldouts, list rows, split dividers, tabs, dropdowns and docking, with
120 ms ease-out transitions on what the pointer touches. Text is 12px Inter on a 16px line. A sheet of your own
overrides any of it, and redefining the `--vx-*` variables is the quickest way to retheme. `Ui::bare()` starts
without it.

**Themes** *(implemented)*: a `Theme` makes every `--vx-*` colour from three choices, as Linear makes its themes: the
base, the ground under every panel; the accent; and the contrast, from 0 to 1. The colours are worked out in OKLCH,
where equal steps of lightness look equal whatever the hue, so any base and accent give a theme that holds together.
- A dark base makes a dark theme and a light base a light one. Panels sit a little lighter than the base, text runs
  from them towards white or black, and each surface and line lies a set distance along the way, so a field is
  always a little further from its panel than a hover, and a line further still.
- Surfaces take a faint tint from the base and the accent, as the default theme's do. The base's share is capped,
  so a strongly coloured base does not colour every word.
- Contrast moves text further from the panels and spreads the surfaces and lines apart. A light theme takes bigger
  steps than a dark one, because small steps in lightness are harder to see near white.
- Colours that mean something keep their hue: success, warning, prefabs and the axes. A light theme makes them
  darker, so they read on white.
- `Theme::DARK` gives the default theme's own colours, close enough that nobody could tell, and `Theme::LIGHT` is a
  pale grey base with a deeper accent that reads on white. `ui.set_theme(&theme)` puts a theme on a UI, after the
  default theme and before your own sheets, so they still win.

```rust
ui.set_theme(&Theme { base: Color::hex(0x1a1612), accent: Color::hex(0xe2a04a), contrast: 0.6 });
```

**The game's UI in C#** *(implemented)*. A script loads a `.vui` document from the project, and its `<Style src>`
sheets from beside it, finds elements by name, and changes them:

```csharp
var hud = UiDocument.Load("UI/Hud.vui");      // null if it cannot be read, with the reason in the Console
var score = hud.Find("score");                // null if there is no such element
score.Text = "0";
hud.Find("bar").AddClass("full");             // RemoveClass, ToggleClass and HasClass too
hud.Find("pause").Clicked += () => Paused = true;
hud.Find("volume").Value = 0.8f;              // a Slider's value; Checked for a Toggle
hud.Find("hint").Visible = false;
hud.Close();
```

- The game's documents live in a Voxear UI of their own, with its own glyph atlas, so a game's sheets never touch
  the editor's and the editor's never touch the game's. The default theme is on its root as on any other.
- It is laid out for the game's area and drawn there, under the window's own UI and over the game's view: the
  whole window in a player, the Game view in the editor while it plays. The renderer draws both in one pass, each
  with its atlas and moved to its area (`Renderer::set_game_ui`).
- The pointer and keyboard reach it in the Game view while playing, and in a player when the fly camera is not
  looking around.
- `Clicked` and `Changed` handlers run at the start of each frame, before the systems update, and a handler that
  throws is logged without stopping the others. `WasClicked` and `WasChanged` ask the same without a handler.
- **Built in code** *(implemented)*, as Unity's UI Toolkit builds it: `UiDocument.Create()` shows an empty
  document, and `new Panel()`, `Label`, `Button`, `Toggle`, `Slider`, `TextField`, `ScrollView`, `Foldout`,
  `Dropdown` and `Icon` are added with `Add`, which gives the element back to keep. An element made with `new` lives
  in C# until it is added to one on screen; what was set on it meanwhile (text, classes, style, visibility, value)
  is sent then, and its children follow (`Ui::create`, `voxear_ui::ElementKind`, the `ui_create` call). `Remove`
  takes one off with what is in it, keeping its text and value so it can be added again. The kinds have their own
  events with the new value (`Toggled`, `Moved`, `Typed`, `Picked`), raised from `Changed`.
- Stopping play takes the game's UI away; reloading scripts forgets the elements they held.
- A built game packs every `.vui` document with the sheets it names (see the content build), since scripts load
  documents by a path the build cannot see. A sheet that is not there stops the build, naming both files.

**Still to come.**
- Editor extensions written in C# with the same API.
- Data binding to C#, using source generators instead of reflection.
- A live-reloading UI builder in the editor.

**Crisp text** uses two paths, the same split Chrome uses:
- **Screen-space and editor text** is rasterised at the exact physical pixel size with hinting. It goes into an
  atlas keyed by font, size and glyph, with the baseline snapped to whole pixels, and a coverage curve tuned for
  light-on-dark text is applied. This is what makes small editor text sharp. *(Implemented, with glyphs on whole
  pixels; 4 sub-pixel x positions come with light hinting.)*
- **World-space text** is drawn from distance fields, so it stays sharp at any distance and angle. *(Implemented
  for UI panels:)* each glyph is made once, at 48 texels to the em, as a single-channel distance field: drawn twice
  as fine, its overlapping outlines filled as one, and measured with an exact distance transform
  (`crates/voxear_ui/src/fields.rs`). A scalable `Ui` draws its text as `Quad::FIELD_GLYPH`s. Each view draws a
  panel's glyphs straight onto it, after the panel's texture (`uiPanelTextVertex` and `uiPanelTextFragment`),
  reading the fields for each pixel on screen: once, its edge soft across a pixel from how fast the distance
  changes on screen, where the text shows at least as large as its fields; otherwise up to four times each way
  across the pixel, averaging how much each covers, so small and slanted text keeps its strokes. Text that
  something later in the panel overlaps, as an open menu, stays in the texture under it. A glyph magnified far
  past its field's size rounds its sharpest corners; fields of three channels would keep them.

**Rendering.** Shapes are instanced SDF quads, so rounded rectangles, borders, shadows and gradients stay crisp at
any DPI. World-space panels render to a mipmapped texture sized to their on-screen pixel density, then draw as
surfaces in 3D. *(Implemented as the UI Panel component:)*
- Each panel's document is a `Ui` of its own (`crates/voxear_engine/src/ui_panels.rs`), sharing the screen UI's
  fonts, its script numbers in a range of its own. Its layout is its size in metres times its pixels per metre,
  and the same at whatever size it is drawn: its `Ui` is scalable (`Ui::set_scalable`), so text wraps alike near
  and far, and nothing moves when the size it is drawn at changes. Its text's lines are not rounded to whole
  pixels, its glyphs are distance fields where its layout puts them (see **Crisp text**), and its edges and
  borders are where its layout puts them, not rounded to whole pixels.
- It is drawn at the size it showed in the last frame's views, which the renderer reports back, in steps of √2:
  it grows at once and shrinks two steps late, so it is not drawn again at every step as the camera moves. Its texture
  is as wide as that step makes whole and as high as its layout then needs, rounded up; its UI is laid out at
  the panel's own size (`Ui::set_layout_size`), and the renderer draws the texture as far past the panel as it
  reaches (`UiPanelFrame::drawn`), so the UI keeps its shape rather than stretching to fill it.
- The renderer (`crates/voxear_render/src/ui_panels.rs`) draws each panel whose quads changed into an RGBA16F
  history image, with the UI shader, before the views, then makes its mips in a compute pass that averages
  premultiplied colour.
- Each view draws its panels after post-processing and before debug draw, so their colours are as designed and TAA
  does not soften their text. It projects them unjittered, samples them anisotropically with a clamping sampler
  (`samplers::ANISOTROPIC_CLAMP`), so a panel seen at a slant stays sharp across the way it is squeezed, and
  discards where the scene's depth is in front.
- The pointer works the nearest clickable panel that a camera ray meets before any collider: through the cursor,
  or through the view's middle while the cursor is locked. Clicks a panel takes do not reach the game's input.
- A panel's document loads the first time a script asks for it (`ScriptServices::ui_panel_document`), so it is
  there in Awake and Start, before the panel is first drawn. A panel on an Obj switched off keeps its `Ui`, and
  so its elements' numbers, but is neither drawn nor pointed at.
- A saved document is loaded again in place (`UiNumbers::replace`): each element scripts hold keeps its number,
  which now stands for the new element of the same name, and elements made in code move across, so handlers
  scripts added keep running.
- `OnUiBuilt(UiDocument)` is a lifecycle phase: after each batch's Starts, and at each frame's start after the
  flush, where the behaviour looks whether its panel's or Screen UI's document is one it has not been given yet.
  A new document, or a behaviour made again by a reload of the scripts, gets it once.

**Screen UI** *(implemented)*: a component (`crates/voxear_engine/src/screen_ui.rs`) that shows a `.vui` document
over the game's view while the game plays, as a document of the screen's own `Ui`, so scripts reach it as one
they loaded.
- `GameUi::sync_screens` runs after each frame's scripts: it loads a new Screen UI's document, closes those
  whose Objs went, hides a switched-off Obj's document and keeps its numbers, and raises the documents by
  `order`. A script asking for `ScreenUi.Document` loads it first (`ScriptServices::ui_screen_document`).
- The screen's `Surface` keeps each document's file, so `GameUi::reload_documents` loads every one again in
  place when its file changes, keeping its numbers and its place among the others.

**Styles from scripts**: `element.Style` sets one inline property at a time (`ui_style_property`, native API 65,
`Ui::set_inline_property`), in place of that property and keeping the rest, the markup's `style` attribute's
too. Voxear.Core writes each typed value as CSS (`StyleCss`), and keeps what was set to send again when an
element made in code comes on screen. `SetStyle` still takes CSS text, replacing the whole inline style.

**LocalizedLabel**: a key's text with `{0}`, `{1}`… filled from code, as a row of labels and prompts. An action's
place is written into the formatted text as a private-use mark, so plurals and currencies format as ever, and the
text is split at the marks. The row is made again only when its shape changes; otherwise its labels' texts
change. Prompts follow the device and rebinding as every prompt does.

**UI shaders** *(implemented)*: `shader: shimmer` or `shader: url("Shaders/Glow.shader")` draws an element's
background with a shader, on the screen, on panels and in the editor alike, since all of them draw through
`uiFragment`.
- `voxear_ui` knows shaders only as a table, `UiShaders`: each one's number and its properties' names, kinds and
  defaults. The engine fills it from the `ShaderRegistry` and the built-in effects (`shaders::ui_shaders`), for the
  screen's UI, every panel's and the editor's own. A name not in it draws the plain background and warns once.
- `Draw::node` marks the background's SHAPE quad `Quad::SHADED`, with the shader's number in flag bits 8–23. A
  SHAPE quad's `uv` is otherwise unused, so it carries where the element's values start, the display scale and the
  opacity, and `Quad` stays 96 bytes. The values, one `[f32; 4]` per property, come from the element's custom
  properties of the same names, or the defaults, and go in `UiFrame::values`.
- The renderer writes each layer's and each panel's values into a per-frame buffer (`Buffers.uiValues`), giving the
  shader where they start in `push.maxDraws` and the time in `push.output`. A panel with a shaded quad is drawn
  again every frame, so its effect moves.
- The built-in effects (numbers from `0x8000`) are in `ui_effects.slang`, always compiled in. A project's UI shaders
  are `.shader` assets with a `ui` function: `ui_dispatch_code` writes them, their property loaders and
  `customUi` into `project_ui_shaders.slang`, compiled into the `ui` module with the material shaders
  (`CustomCode`), hot-reloaded in the editor and shipped as `Shaders/ui.spv` in builds. Until then the stub in
  `ui_custom.slang` draws the plain background. Material dispatch leaves UI shaders out, and a material that picks
  one draws Standard.

**The editor is built with Voxear UI**, so every editor improvement also improves game UI.

## Editor

**What exists now** *(first slice, in `crates/voxear_editor`)*: `vox editor <project>` opens the project's start
level in **edit mode**, in the editor's design (below), built in Voxear UI:
- **Edit mode:** Objs load with their behaviours kept as data (`EditBehaviours`), so no script runs and a project
  opens without its scripts built, as in Unity.
- **Play mode**, as in Unity. Play copies the open level, edits included, into a fresh engine whose behaviours
  run (`Levels::play_copy`: each loaded scene is captured from the edit world with its Objs' own ids, so persistent
  Objs are still recognised across level changes, and spawned again), and brings the Game view to the front. Stop
  destroys the played world, so behaviours get OnDisable and OnDestroy (`Engine::shutdown`), and swaps the edited
  level back exactly as it was. The selection follows between the two (`Levels::counterpart`), and the Scene view
  keeps where its camera was. Pause holds the game still and Step moves it on one frame. The status bar says
  whether the editor is building scripts, playing or paused.
  - The scripts build (`voxear_engine::scripts`) off the main thread the first time Play is pressed, then .NET
    starts with them. .NET starts only once per process, so the scripts are kept from run to run, and their static
    state carries over, as in Unity with domain reload turned off, until they change.
  - **C# hot reload.** When a script changes, the editor builds the scripts again in the background (as it does to
    show their fields), and they reload: while editing at once, and while playing into the running game, which keeps
    going, unless the settings say to wait for play to stop. Play also builds and reloads scripts changed since the
    last build. The scripts are read into memory rather than mapped from their files, so a build can write the next
    version while one runs. A reload (`Engine::reload_scripts`, then `GameScripts.Reload` in Voxear.Core) loads the
    new build into a new collectible load context, gives every enabled behaviour OnDisable, makes each behaviour
    again from its class in the new build under the same handle, and gives the enabled ones OnEnable; Awake and
    Start do not run again, and the lifecycle takes each class's new callbacks and execution order. Every field
    carries over by name (`StateTransfer`), private ones and those behind auto-properties too, as Unity's reload
    keeps them: values of the scripts' own types are made again as the new build's types of the same names (classes
    and structs field by field, enums by name, arrays, lists and dictionaries item by item, references to
    behaviours as their replacements), shared and circular references stay so, and a number or text whose field
    changed type is converted. What cannot be carried, such as a delegate into the old code, starts as the new class
    sets it, and the Console names it. A behaviour whose class is gone is dropped from its Obj, with a warning.
    Static fields start afresh, listeners the old build added to `Level`'s events are dropped, and awaits it was in
    the middle of never resume. The old context is then unloaded, and a couple of seconds later the editor checks
    that it left memory, warning when something (a thread or timer of the old code, say) still holds it. A build
    that fails to compile leaves the old scripts running, and the compiler's errors in the Console. `vox run`'s
    window reloads changed scripts the same way (`Game::watch_scripts`, which builds them off the main thread as
    they change). A build of the scripts that runs past ten minutes, or describing one past two, is taken as stuck
    and stopped, so it cannot hold Play up for good.
- **The top bar** has the Voxear mark, the **Layout** menu, the command bar (Ctrl+K), play, pause and step in a
  pill, the **Window** button and the Build button. Window lists every panel, ticked while open, to open or close,
  and resets the layout.
- **Layouts** (`editor::windows`), as Unity's: arrangements of the same panels, picked from the Layout menu, which
  names the one showing. Default is for building levels; Profiling has the Game view over the Profiler, the
  Entities panel, the Frame Debugger, the Memory panel and the Console. Save Layout As keeps the arrangement
  showing under a name typed in a box under the menu's button, and Delete Layout forgets a saved one. Each layout
  keeps the arrangement it was left in, as Blender's workspaces do, and Reset Layout puts it back as it started.
  `Editor::use_layout` and `layout_name` pick and name them from code.
- **The rail** down the left opens a **drawer** over the left third of the dock: Levels from the top, Settings
  from the bottom. Its button again, or the drawer's close button, closes it. The palette's commands that show the
  Levels or Settings open it there.
- **The command palette** (`palette`) opens under the command bar when it has focus (Ctrl+K). It holds the editor's
  commands with their shortcuts (play, save, undo, duplicate, the Scene view's tools, new Objs, showing each panel),
  commands added by code, the settings, every Obj (selecting it), the levels (opening them), the project's assets
  (showing them in the Project) and the VoxDocs pages. Matching is fuzzy: the letters typed need only come in
  order, and a result ranks higher the more of them start words or follow one another, then the shorter it is. What
  was run is remembered per project, by a key that stays the same from run to run (an Obj by its scene and its place
  there): with nothing typed the recent come first, then the commands, and typed for they rank higher. Up and Down
  move the highlight, Enter or a click runs it, and Escape closes the palette and empties the bar.
- **Adding to the palette.** Game scripts mark a static method that takes and returns nothing with
  `[Command("Swap Spinners", Keys = "Ctrl+Shift+X")]`, as Unity's `[MenuItem]`; the scripts' description lists it
  (`script_types`), so it is in the palette, and its keys run it from anywhere in the editor. It runs in edit mode,
  against the level being edited: .NET starts for it the first time (building the scripts if need be), the method is
  called with the edited world lent (`ScriptRuntime::run_command`), and what it changed, renamed, moved or destroyed is
  one step to undo. Scripts find Objs with `Obj.Find` and `Obj.FindAll`. Behaviours do not run in edit mode, so a
  command finds none; while playing, a command runs against the game. Rust code built on the editor adds commands
  with `Editor::add_command(PaletteCommand { id, title, keys, icon, run })`, `run` getting the editor.
- **Settings** (`settings`): the Scene view's snap distance, handle position (pivot or center) and space (global or
  local), fly speed, and whether the stats chip and axis gizmo show; whether Play starts paused, saves first, clears
  the Console, pauses on an error, and reloads changed scripts while playing; the Scene view's shading and gizmos;
  and the Console's Collapse and trace toggles, and which code editor opens files. They are kept per project in
  `.voxear/Editor.toml` with the palette's recent items and the layouts, the saved ones, each one's arrangement
  and which one showed, so the editor opens as it was closed. They are shown in the rail's Settings drawer, with a tab for
  each group down its left and a search over all of them that matches as the palette does. The tool strip's
  Pivot, Global and snap buttons, the Scene view's header and the wheel's fly speed are the same settings. From the
  palette, a toggle flips, a choice moves to the next, and a number opens the Settings drawer at it, focused to
  type. Tests and screenshots open
  the editor without memory (`Editor::open_without_memory`), so nothing carries from one run to the next.
- **Docked panels:** the Hierarchy (each open scene under its own header, its Objs by parent in the scene's order,
  with type icons, child counts, a badge on Objs with an `IInstance` behaviour, prefab instances tinted, and a
  filter), the Scene view and
  the Game view (through the level's first camera; both draw at once when both show, as in Unity), the Project and
  the Console under them, and the Inspector on the right at 300px. Each panel has a
  32px header of tabs (the Scene's and Game's with icons) and its own controls on the right: the Scene's are its
  shading menu, Gizmos and full screen. They can all be dragged around, as docking allows. Every window's options
  button, at its header's right end (the Game and Scene views' too), and a context click on a tab or its bar,
  offer Maximize (or Restore), Close, Close Other Tabs, Float or Dock, Float in Its Own Window, Add Hierarchy, Add
  Inspector, Add Scene View and Add Project (another of the windows there can be more than one of, as Unity's Add
  Tab), and Reset Layout; a closed panel keeps its state, and opens again from the Window button, beside a panel it
  shared an area with.
- **Windows of the desktop's own** *(implemented)*. Float in Its Own Window, or a tab or floating window let go
  past the main window's edge, takes a panel out of the editor's window into a desktop window of its own (see
  the UI's desktop windows). The editor asks for the windows the dock wants (`Editor::wanted_windows`), and its
  window code opens a winit window for each, with an output of the renderer, and hands it over
  (`open_desktop_window`); a test does the same with offscreen outputs. Each frame the editor gives each window its
  UI, draws each Scene or Game view into the window it is in, and the game's own UI over the Game view wherever it
  is. Events come in with the window they came from (`handle_in`), and the right button flies a Scene view in any
  window. Closing one from its frame closes its panels, as Close does, and a panel dragged out of it or docked
  again with Dock takes it away once it is empty. The layout keeps it, with its size, so it opens again next time.
  Its title is the tab showing and the project's name, and its app id is `voxear-editor-panel`, for desktops'
  rules about which windows float.
  - While the left button is held, the window it went down in keeps the pointer's moves wherever it goes. On X11
    and Windows, which say where windows are, a move over another of the editor's windows is handed to that
    window, from its own top left, so a tab drags from one window to another with the dock's preview showing where
    it lands; a window torn off opens under the pointer, and opens where it was left next time.
  - Wayland never says where windows are, so there a tab dragged out past its window's edge becomes the desktop's
    own drag and drop (`wayland_drops`), of a type of the editor's own (`application/x-voxear-panel`). The window
    it passes over hears it from its own top left, so its preview shows, and a drop there docks it; let go
    anywhere else, it gets a desktop window of its own. The desktop places new windows itself, and a press on a
    desktop window's bar asks the desktop to move it (`xdg_toplevel.move`), as its own title bar would.
- **The Project** is laid out as Unity's two-column Project window: the folders as a tree on the left, and the open
  folder's folders, assets and C# scripts on the right, with a breadcrumb and an item count. With room for three
  or more, they show as tiles; narrower, as rows with each file's size. Selecting a folder in the tree opens it,
  as does double-clicking one on the right. The Project sits under the Scene by default, because its two columns
  need the width; dragged under the Hierarchy, it turns to rows. A model has an arrow that opens it out, as Unity's
  does, to its meshes (named as the model names them) and then its materials, after it as tiles or rows of their
  own; the editor reads them from the model the first time (`ProjectEvent::WantsParts`, `objs::model_parts`). A
  part drags onto a field that takes it, a mesh onto a Mesh Renderer's Mesh and a material onto its Material, as
  `Models/Crate.gltf#mesh0.0`. With the Project panel setting on, a prefab opens out to its Objs too.
- **Working with the project's files** (`project_files`). A context click on an item offers Open, Show in Files,
  Copy Path, Rename, Duplicate and Delete, and on the empty space or a folder, a new folder, scene, level, material
  or C# script, named so as not to clash ("New Scene 1"). Renaming takes the item's `.meta` along, so its GUID and
  every reference to it hold, and renaming a script renames the class it declares; renaming is typed into a bar
  over the files. Duplicating copies a file without its `.meta`, so the copy gets a GUID of its own, and deleting
  moves a file and its `.meta` into the project's `.voxear/Trash` rather than destroying it. The panel looks at
  the folder again after each (`Content::rescan`).
- **The Inspector for assets** (`asset_inspector`). Selecting an item in the Project shows it in the Inspector, in
  place of the Obj selected, as in Unity: its icon, name and kind, where it is (path, GUID and size), and what its
  kind holds. A material's colour, metallic, roughness and emission are edited in place and saved to its file,
  showing at once wherever it is drawn (`Content::save_material`, `Renderer::update_material`); its colour and
  emission have swatches that open the colour picker, the emission's with its HDR intensity; a script shows its
  behaviour's fields with their starting values; a prefab or scene lists its Objs. A scene also shows its lighting
  as the Lighting page shows the open scene's (`LightingPanel::show_scene`): an open scene's change goes through
  `Levels::set_lighting_of` and is saved with it, and one not open has its `lighting` written into its file. Levels,
  scenes, prefabs, scripts and input actions have an Open button.
- **A level's scenes in the Inspector** (`level_files`). A level shows the level it stands on and its own scenes,
  each a reference to a scene, with an Active box (one is always active: the one marked, or the first), a Required
  box and a button to take it out, and **Add Scene**. As an Obj's references do, a scene's field pings the scene in
  the Project on a click, opens the object picker from its arrow to put another scene in its place, and takes a
  scene dragged onto it from the Project; Add Scene opens the picker on the project's scenes the level does not
  list, and takes a dragged scene too. A scene the level has already, or anything that is not a scene, is turned
  away with a note saying why. Each change is written to the
  `.level` at once, as a material's are, through `toml_edit`, so its comments and layout stay; a scene added is
  written by its GUID with its path in a comment. The Levels panel lists again, and a level open is opened again
  (`Game::load_level_with`), so a change to it or to a level it stands on shows in the Hierarchy at once: the scenes
  it keeps stay as they are, unsaved edits included, and only those added load and those taken out unload. Taking
  out a scene with edits not saved asks for a second click first, and clears undo, whose steps may name its Objs.
  Not while playing.
- **Scenes alongside the level in the editor** (`editor::scenes`): a scene's menu in the Project has Open
  Additively, and a scene dragged onto the Hierarchy or the Scene view opens the same way. Its header in the
  Hierarchy is marked "Not in level" while a level is open that does not list it, and its menu offers Add to the …
  Level, which writes it into the level's file as Add Scene does; any header's menu, while more than one scene is
  open, offers Close Scene, which asks for a second click when edits are not saved and clears undo. Saving writes
  every open scene that changed. Not while playing, nor in prefab mode.
- **The Scene view** renders into its panel through the editor's own camera, flown with the right mouse button
  as in Unity. The renderer draws the 3D view into a rectangle of the window (`Renderer::set_view_rect`), clears
  around it to the window's background (`Renderer::set_clear_color`), and looks through a chosen camera
  (`ViewOptions::camera`). More views draw in the same frame, each into its own rectangle through its own camera
  (`Renderer::set_more_views`), so the Game view draws beside the Scene view, or over it from a floating window:
  the editor draws the views in floating windows last, and makes each view's element see-through
  (`ui.set_see_through`), so the Scene view's gizmos under a floating Game view do not show through it. A frame
  drawn over its edge rounds its corners to the panel's. Over it float a tool
  strip (select, move, rotate and scale, Pivot, Global and grid snap), a stats chip, and an axis gizmo that turns
  with the camera. The header's **shading** menu draws it Lit, Unlit (base colours only), as a Wireframe, or as Lit
  Wireframe, the lit scene with its meshes' edges drawn faintly over it (the renderer's debug views; the Game view
  is always lit). **Gizmos** (`gizmos`) draws a sun for each light and a
  camera body for each camera, which a click selects, and for the one selected, a light's reach (its range, or its
  spot's cone) and a camera's frustum; the button shows or hides them, and the menu beside it lights', cameras' and each
  behaviour type's (settings `light_gizmos`, `camera_gizmos` and `hidden_gizmos`, for every Scene view). **Full screen**
  gives the dock to the Scene view alone until it is pressed again.
- **Several Scene views** (`editor::scene_views`), as Unity's: Add Scene View on a window's menu opens another
  beside it, looking from where the Scene view in use looks. Each is built from `SceneView.vui`, looks through a
  camera of its own, which the Hierarchy leaves out and which flies on its own, and has its own shading menu and
  Gizmos button, so one can show Wireframe while another shows Lit; the first one's shading and Gizmos are the
  settings, and the tool strip's settings are shared. They draw at once, in the order they were opened, so each
  keeps its own history in the renderer. The tools act in the one the pointer is over: that one is swapped in as
  the editor's `scene`, so flying, handles, a click to select and a prefab dropped all go to it, and the others
  draw their gizmos and the selection's handles as they are. Prefab mode's bar shows over every Scene view. While
  playing, each flies a camera of its own in the world playing, and its camera in the edited level takes where it
  flew to when play stops. One opened from the menu goes for good when it closes, its camera with it. Layouts keep
  the ones they show: the dock names each by its panel (`scene-2`), and the editor's memory keeps where each one's
  camera was and how it was shaded (`Memory::scene_views`), so after a restart, or on going back to a layout that
  names one, it opens again in its place and as it was left. A layout that leaves one out, Reset Layout included,
  takes it away for good, as closing it does (`windows::arrange`).
- **Several Objs selected** *(implemented)* (`editor/selection.rs`): Ctrl and a click in the Scene view or the
  Hierarchy adds an Obj or takes it out, Shift adds (or in the Hierarchy selects the rows between), and a box
  dragged over the Scene view from a press on nothing selects what its bounds on screen touch. The first selected
  is the active Obj, which the Inspector shows and the handles sit on; the others follow what a drag does to it,
  its change in the world applied to each about where the active Obj stands, and the drag is one `Edit::Group`
  to undo. Delete and Duplicate act on the outermost of them, and each one's bounds are drawn while there are
  several. An Inspector not locked edits them all (`Inspector::set_also`): it shows only the sections every one
  has, "—" in a field (or an axis of one) they hold differently, with `.mixed` on its row, and a value changed
  goes to each, a vector's or a Transform's only on the parts edited, as one step to undo.
- **Scene view tools** (`scene_tools`), as Unity's. A click selects the nearest Obj under the pointer: a ray
  against each mesh's bounds in the Obj's own space (`Renderer::mesh_bounds`; cooked meshes keep only a sphere,
  so they use the box around it), or a light or camera close to where it shows; a click on nothing clears the
  selection. Around the selected Obj, move has an arrow per axis, a square between each two axes that moves along
  both, and a middle that moves in the view's plane; rotate a ring per axis, an outer ring that turns around the
  direction the view looks, and a faint ball inside the rings that turns freely, its near side following the
  pointer; and scale a block per axis, a square between each two that scales both together, and a middle that
  scales evenly. The squares sit on the side of each axis that faces the camera, as Unity's do, and hide when seen
  nearly edge on. Handles are Voxear UI shapes
  projected through the same matrices the renderer uses (`ViewCamera`), stay the same size on screen, and light
  up under the pointer, which takes a press on one before the UI sees it. The strip picks the tool (or Q, W, E and
  R), Pivot or Center, Global or Local axes, and the snap distance (0.1, 0.25, 0.5 or 1 m); Ctrl while dragging
  snaps to it, to 15 degrees, or to tenths of the scale. A drag is one step to undo.
- **Making, duplicating, deleting and reparenting Objs** (`objs`), from the Hierarchy. Its + button makes an Empty,
  a Cube, Sphere or Plane, a directional, point or spot light, or a camera, in front of the Scene camera on the snap
  grid. A context click on an Obj offers Duplicate *(Ctrl+D)*, Delete *(Delete)* and the same new Objs made inside
  it, and Open Prefab first on a prefab instance or an Obj in one; on a scene's header or the space below the rows, new Objs at the top of that scene. Dragging an Obj keeps
  where it is in the world, and lands by where the pointer is on the row under it: on the middle, inside it, last;
  near its top or bottom edge, beside it, before or after (a line shows which); on a scene's header, at the top of
  that scene; and below the rows, at the top of its own scene. Copies are named as Unity names them ("Cube (1)")
  and go just after the original. Objs are copied, and brought back by undo, through scene files of them
  (`scene_file::capture_scene` and `spawn`), so everything a scene holds comes back, prefab instances included.
- **Sibling order is the scene's order.** A scene keeps its Objs in a list (`Levels::scene_order`), which its file
  writes in that order and the Hierarchy shows in that order, so reordering needs nothing more in the format and
  saves as shown. Each Obj's place there is kept with it (`Tracked::index`): Objs moved (`Levels::move_objs`) go
  to their new place as a block with what is under them, and Objs untracked and tracked again, as when a delete or
  a move is undone, go back to the places they had. New Objs go to the end of the active scene, or their parent's,
  with ids after the highest there; Objs moved into another scene move into its list with new ids; deleted Objs
  come back with the ids they had. While playing, these change only the played world.
- **The Inspector** has a short header, as Unity's: the Obj's icon, its name and an active switch, then Tag and
  Layer side by side, then Static and Persistent as checkboxes on one row (Tag, Layer and Static shown, not yet
  stored: Objs have no tags or layers yet). Persistent marks the Obj to stay through
  level changes (`ScenePersistence`, saved as `persistent = true`) as a step to undo. The Transform and each component
  are sections divided by lines, each headed by its icon and title with an enabled switch (or, for the Transform, a
  menu) on the right, and with an 80px label column; position, rotation (in degrees) and scale are mono fields with
  coloured axis letters, applied as they are typed, and changes made elsewhere show in fields not being edited. While
  playing it shows the running Obj, its behaviours included.
- **Several Inspectors** (`editor::inspectors`), as Unity's: Add Inspector on a window's menu opens another beside
  it, and each has a padlock that keeps it on what it shows while the selection moves on, as to compare two Objs.
  The selection is the editor's own, which every Inspector not locked shows. Each runs the same Inspector's frame in
  turn, swapped in as the editor's `inspector`, so editing, menus, the object picker, buttons, callbacks and drawers
  act on the Obj it shows; each keeps its drawers apart by an owner number the scripts know. One opened from the
  menu goes for good when it closes, and locked ones follow their Objs into the played world and back. A layout that
  names one (`inspector-2`) opens it again in its place, after a restart or on going back to that layout, showing
  the selection; a layout that leaves it out takes it away for good, as Scene views opened from the menu are.
- **Several Hierarchies and Project panels** (`editor::hierarchies`, `editor::projects`), as Unity's: Add Hierarchy
  and Add Project on a window's menu open another beside it, each built from the same markup as the first
  (Hierarchy.vui, Project.vui). Hierarchies list the same Objs with their own filter and folds, and share the
  selection: a click in one selects in all, and a ping lights the Obj in each. A click on the space below the rows
  selects nothing. Project panels each show a folder of their own, so two can be open side by side to drag between;
  files that came, went or were renamed show in every one, and their previews are drawn for each. Each runs the
  same frame in turn, swapped in as the editor's `hierarchy` or `project`, so its menus, drags and drops act as the
  first's do, and what is dragged from any of them, or dropped on any, is heard. One opened from the menu goes for
  good when it closes, and a layout naming one (`hierarchy-2`, `project-2`) opens it again.
- **Components in the Inspector** (`props`): each component is a part, and each value in it a prop, read from the
  world and written back as it is typed or picked. A Mesh Renderer picks its mesh (cube, sphere, plane, or the model
  mesh it has) and its material (the default, or a `.material` in the project, which can also be dragged onto it from
  the Project) in the object picker; a Light its type, colour (a swatch, which opens the colour picker under it,
  and sRGB channels), intensity, and the range and spot angles its type uses; a Camera its field of view, near plane
  and exposure. **Scripts** show the fields their build has (`voxear_engine::script_types`): when a script changes,
  the editor builds the scripts off its main thread and runs `Voxear.ScriptInfo` on the build, a small .NET tool
  beside Voxear.Core that reads the assembly with reflection, under the same rules that set fields when the game
  runs (`BehaviourFields`), and makes each behaviour to read the values its fields start with. Its description,
  `.voxear/Scripts/Behaviours.json`, is kept, so a project opens showing its fields at once. Build errors go to the
  Console, and the fields stay as they last built. Fields are labelled as Unity labels them ("Degrees Per Second"):
  numbers, whole numbers, text, toggles, enums (a dropdown) and vectors are edited in place; an Obj or behaviour
  reference is picked in the object picker from the Objs in the same scene (with that behaviour, for a behaviour),
  and an asset (a `Prefab` or `Ref<Prefab>`) from the project's assets of its kind, or either is dragged onto the
  field from the Hierarchy or the Project, which takes it if the picker would offer it and otherwise says why in the
  status bar; a list shows its items, each with a button
  to take it out and the list's to add one; and a `[Serializable]` table shows its fields indented under it. A field
  the scene does not set shows the script's value, and setting anything in it writes the whole field for the scene.
  While playing, a running behaviour's fields are read from its instance each frame and show as they are now, a
  value changed in the Inspector is given to the instance (and goes with the played world when play stops, unless
  kept), and its `[Button]`s and `[OnValueChanged]` callbacks run on it (`LiveFields` in Voxear.Core, through the
  Inspector's JSON calls). **Add Component** is a search in the object picker, in folders as Unity's: the
  built-in components the Obj lacks in Rendering and Audio, and the project's scripts under Scripts, a folder for
  each part of their namespace past the one all share, or where `[AddComponentMenu]` puts them (the catalog's
  `menu`; empty leaves one out). Picker rows say what picking does (`Chosen`: a value, a component, or a folder to
  open), and with nothing typed the open folder lists its folders, then what is in it; typing searches every
  folder. A context click on a component's header removes it, or moves a
  script up or down among the Obj's scripts, which is the order they run and save in, and on a script field's row
  resets it to the script's value. Every change is a step to undo.
- **Reference fields**, as Unity's object fields: a click pings what one points at, lighting an Obj's row in the
  Hierarchy (its scene and parents opened) or an asset's tile in the Project panel (its folder opened); a double
  click selects an Obj it points at; and its arrow, a button of its own, opens the object picker.
- **The object picker** (`object_picker`), as Unity's: a reference field's arrow opens it under the field, titled
  for what the field takes ("Select Material", "Select Spinner"), with a search box to type in at once over None and
  everything the field can take, each with its icon and where it is (an asset's folder, an Obj's scene). Typing
  narrows it as the command palette does, letters in order and the best match first, by name or by place. Up and
  Down move the highlight, which starts on what the field points at now; Enter or a click picks, as a step to undo,
  and Escape or a click elsewhere closes it. A Mesh offers the built-in shapes, then each mesh of every model
  Content has loaded (`Content::loaded_model`), named `Gate #mesh0.3` beside its model's file, and each model not
  loaded yet whole, which picked loads and gives its first mesh; listing loads nothing, so it opens at once.
- **Previews in the editor** (`editor::previews`): materials, prefabs and models show previews of themselves in
  place of their icons in the Project panel's tiles and rows (only the tiles on screen are asked for) and in the
  object picker's rows; an asset field shows a thumbnail of what it points at before its name, and a Mesh
  Renderer's Mesh and Material fields show its own mesh and material. Selecting a material, prefab or model in
  the Project shows a large preview in the Inspector, which a drag turns. New previews start four a frame, so a
  folder of models opens without a hitch; saving a material draws its previews again, and files moving makes
  them anew.
- **Picking colours** (`editor::color_popup`): a click on a colour's swatch opens the colour picker under it, for
  a Light's colour or a material's Colour and Emission (HDR, with its intensity). What is picked shows at once; an
  Obj's changes over the whole visit are one step to undo, and a material is saved as it changes. A press outside
  it, or Escape, closes it.
- **Keeping values after play.** A context click on the name, a Transform row or a component's value opens its menu:
  Reset where there is one, and while playing, **Keep value after play**, whose row is then marked in the accent.
  When play stops, each kept value of an Obj from the edited level (`Levels::counterpart`) is copied into the edited
  Obj, as one step to undo, and the status bar says how many were kept.
- **Saving** *(Ctrl+S)* writes the open level's scenes back to their files (`Levels::save`). Only scenes that changed
  are written, so untouched ones keep their comments; each keeps its own sky, prefab instances keep their paths and
  write only what differs from the prefab, and Objs made since the level opened go into its active scene. Hello's
  levels save back unchanged. Edits while playing are not saved, as in Unity. Opening another level with unsaved
  changes asks for a second click first.
- **Making prefabs.** Dragging an Obj from the Hierarchy onto the Project panel writes it, with what is under it,
  to a `.prefab` (`objs::make_prefab`) in the folder let go on: a folder's tile or a folder in the tree, which light
  up while an Obj is over them (`ProjectView::take_drops`), or else the folder shown. An instance of the new prefab
  takes its place, as one step to undo. An instance shows as one: in the Hierarchy a prefab cube in the prefab tint,
  and in the Inspector a Prefab row (In prefab, for an Obj inside one) naming the prefab, whose Open opens it in
  prefab mode and Select shows it in the Project and the Inspector, as Unity's prefab bar does.
- **Placing prefabs and models** (`editor::placing`, `objs::place_prefab` and `place_model`). A prefab dragged from
  the Project places an instance of it, and a model an Obj drawing it, with a Mesh Renderer on its mesh and its own
  material (`<model>#mesh0.0`, `#material0`); a model of several parts places an Obj with one inside for each part,
  where the model puts it. Either, dragged onto the Hierarchy, lands where an Obj moved there would: inside the Obj
  under the pointer, beside it near its edge, at the top of a scene on its header, and below the rows at the top of
  the active scene. On the Scene view it lands where the pointer's ray first enters a mesh's box, or else meets the
  ground plane (y = 0, within 200 m), snapped to the grid across; failing both, 8 m along the ray on the grid. Inside
  an Obj, what is placed keeps the position its prefab or model gives it; elsewhere that position is an offset from
  where it lands, so a prefab made to stand on the ground at the origin stands on it. What is placed is named for the
  prefab or model, numbered as copies are when the name is taken, selected, and one step to undo. Only while a prefab
  or model is dragged do the two take drops, so nothing else lights them up; the Scene view is tinted in the accent
  under it. In prefab mode, what would land at the top goes inside the prefab's root, keeping its place in the world,
  as a prefab has one root; and the prefab open, or one that holds it, is not placed.
- **Materials dropped on Objs** (`editor::material_drops`), as in Unity. A material from the Project (or a model's,
  from the opened-out model) held over a Scene view shows on the Obj with a Mesh Renderer under the pointer, picked
  as a click picks; moving off, or the drag ending elsewhere, puts the Obj's own back. Let go of there, or on an
  Obj's row in the Hierarchy, the Obj takes it through the same row action as the Inspector's object picker, so it
  is one step to undo; an Obj that draws nothing says so in the status bar.
- **Prefab mode** (`editor::prefab_mode`), as in Unity: a prefab opens on its own to edit from the Project (a double
  click, or the Inspector's Open), or from the Hierarchy's Open Prefab on an instance. Its Objs load as a scene's
  would, in place of the level (`Levels::open_scene` takes prefabs), a bar over the Scene view names it, and Back,
  or Close Prefab at the bar's other end, opens again the level or scene it was opened from. Save writes the prefab, and what was read of it is then
  forgotten (`Content::forget`), so the level's instances show the change when it opens again. A prefab does not
  play on its own: Play asks to go Back first.
- **Locking components for designers.** In prefab mode, a component's menu has Lock for Designers. A locked
  component shows a padlock, and in the prefab every value of it still shows, those designers do not see dimmed;
  a value's menu has Show to Designers. On the prefab's instances, outside prefab mode, a locked component shows
  only the values chosen and cannot be removed, so a programmer can fix how a component is set up and leave
  designers the values meant for them. Locks are saved in the prefab's file, by component (a behaviour's type, or
  `renderer`, `light` or `camera`) with the fields still shown (`[obj.locks]`, the `Locks` component); they are the
  prefab's alone, never an instance's overrides, and changing them is a step to undo.
- **Undo and redo** *(Ctrl+Z, Ctrl+Shift+Z or Ctrl+Y)* take back and make again edits to an Obj's name, Transform
  and components, from the Inspector or the Scene view's handles, values kept from play, and Objs made, duplicated,
  deleted or reparented; typing into one field is one step, and so is a drag. Objs brought back are new entities,
  so every step naming them is pointed at the new ones.
- **The status bar** shows a light, what the editor is doing (ready, building scripts, playing or paused, or that
  there are unsaved changes) and how long the project took to open, then the renderer, frame rate, memory and git
  branch. The numbers are in figures of one width and change at most once a second; the frame rate follows the trend
  and holds still within 3% of what it shows, so it can be read.
- **The Console** (`console`), after Sifter. Each line is a row in columns: when (minutes and seconds since the
  editor started), the message's first line, how many it stands for when collapsed, and where it came from, in mono.
  Each log line carries its level, target, Rust file and line, the frame it was logged in, whether it came from a
  worker thread, and for scripts, the stack .NET wrote and the Obj it is about (`LogLine`; scripts' `Log` sends both
  through `log` in the native API). A line is in categories nobody declared (`console::entry`): a message starting
  `[Combat]` is in Combat, a script's line in its code's root namespace (from its first frame of the project's own),
  an engine line in its crate (Engine, Render, RHI, UI), and the C# compiler's lines in Compiler. The toolbar's
  Categories button lists each category with its count, ticked while shown, to hide or show it, with Show All; the
  menu stays open while categories are picked. The toolbar wraps, so in a narrow Console every control stays on
  screen. The search (`console::query`) takes words, `tag:name` and `-tag:name`, and a regular
  expression between slashes; a bad one says why under the toolbar. Log, Warning and Error toggles count what the
  search and categories pass. Collapse, Clear and Error Pause (which pauses play on an error) are settings, as are
  the trace's toggles. Below the list, the selected line shows when, in which frame and on which thread it was
  logged, its message in its level's colour, and its stack (`console::frame`), each frame parsed from the end, as
  Sifter learned to (the place, then the parameters by counting brackets back, then the method and type; async
  state machines read as the method they are), and coloured by part: namespace, class, method, parameters, file
  and line. My code only folds runs of frames not in the project's folder into a line that unfolds them;
  Namespaces and Parameters can be hidden. A frame's file and line, or a double click on a line, open the file at
  its line in the code editor the settings name (VS Code, Rider, Zed or the system's default;
  `Editor::set_source_opener` replaces it). A script says what a line is about after its message, `Log.Info("Opened", door)`,
  with an Obj, a behaviour, a `Prefab` or a `Ref<Prefab>` (`LogContext`, native API 24, which names a prefab by
  its file); picking the line pings it as Unity does, lighting its row in the Hierarchy or its file in the Project
  panel for a moment, its folders opened and scrolled to, without changing the selection. Copy puts the line and its
  stack on the clipboard, and a context click copies a line or mutes its message until Muted brings them back.
  - **Category colours**: each row shows its first category, in the colour the project gave it. A context click on
    a line colours its category from a palette; the colours are kept in the project's `Settings/Console.toml`, since
    a team should agree on them.
  - **Filters** saves the search, the levels hidden and the categories hidden under a name, ticked while in use, to
    pick again or forget; they are kept per user with the settings.
  - **Hide libraries…** over the trace lists the assemblies in the selected line's stack (a frame's root namespace, such as
    `Voxear` or `System`, or an engine crate), ticked while hidden; their frames fold into a line that unfolds them,
    as My code only does, for the one library that fills every trace.
  - **The player's log**: Run in the Build window starts the built game with its output piped back, and each line
    comes into the Console at its level, in the Player category, ending with how the game closed. A game run on its
    own keeps its log in a file (see "The player"), and **Player Log** in the toolbar lists each build that has
    run, its last run and the one before, to read into the Console the same way.
- **The Profiler** (`profiler`), in the Profiling layout, after Unity's. It keeps the last 300 frames as a chart of
  their CPU and GPU
  milliseconds, under lines at 60 and 30 frames a second, and breaks one frame down. Its **Hierarchy** is the
  frame's samples as a tree, each row with its share of the frame with and without what is in it (Total and Self),
  how many times it ran, and its milliseconds, the slowest first: the editor's stages (Game, Editor with a sample
  per panel, UI with its Styles, Layout, Lists and Draw, and Render, which includes waiting for the GPU and the
  display), the engine's steps inside
  the game's tick (FixedUpdate, Update, LateUpdate, Lifecycle), each system by name, and **each behaviour's
  callbacks by class and method** ("Hello.Spinner.Update()", one row for every Spinner, with its calls), with the
  samples scripts take themselves. Beside it are the renderer's passes as the GPU timed them (`Renderer::timings`)
  and what the frame drew and culled, by LOD, with memory and what is loaded. A click on the chart picks a frame,
  and dragging along it goes through the frames; Live goes back to the newest, and Record stops and starts taking
  frames. The Timeline scrolls when its lanes are more than fit; the wheel zooms it around the pointer (it takes
  the wheel with `Ui::take_wheel` and reads `Ui::wheeled`), a drag moves along the frame, and a double click shows
  it whole. Samples are taken only while the panel
  shows and records.
- **Profiling samples** (`voxear_core::profiling`): named spans of a frame's work, nested as they ran, merged by name
  under the same parent as Unity's Hierarchy merges them. Sampling is per thread and off until the Profiler turns
  it on for the main thread, where scripts run too; a sample then costs a clock read and a short search among its
  siblings. The easy form in Rust is `profile_scope!("Pathfinding")`, which samples the rest of its block; the
  advanced one a `Marker` made once and begun and ended by hand (`profile_marker!` makes one per call site). The ECS
  schedule times each system through a hook (`Schedule::set_system_timer`), since `voxear_ecs` depends on nothing.
  In C#, `using (Profiler.Sample("Pathfinding")) { ... }`, `Profiler.BeginSample` and `EndSample` as in Unity, or
  a `ProfilerMarker` kept in a static field and `using (marker.Auto())`; each behaviour callback is sampled by
  `BehaviourTable.Invoke` while the Profiler takes samples, under a marker made once per class and method.
- `vox editor --screenshot editor.png` renders it offscreen instead, with an Obj selected.

The Odin-style inspector attributes come next.

**Look.** Unity's modern dark concept crossed with Linear:
- Surfaces are separated by elevation rather than heavy borders.
- Themes are generated in OKLCH from three inputs (base, accent, contrast), as Linear does, so light, dark and
  custom themes all stay consistent (see Themes under Voxear UI). The Settings drawer's Appearance group picks Dark,
  Light, or Custom, which is whatever base, accent and contrast were last chosen; a colour opens the colour picker
  from its swatch.
- Inter and Inter Display fonts.
- Subtle 1 px separators.
- Animations take 100–180 ms with ease-out curves and never block input.

**Layout.**
- Dockable tabbed panels: Hierarchy (with instance badges), Inspector, Project, Scene, Game, Console, Profiler and
  Lighting.
- Built-in layouts for Animation and Lighting, when those systems are built.
- Floating tool overlays in the viewport, like the Paint Texture panel in the concept.

**Console.** Modelled on CookieJarTools' [Sifter](https://github.com/CookieJarTools/Sifter), a Unity console
whose source is in `Dev/CookieJar` (design notes in `Docs/Sifter-Design.md`):
- Every part of a stack trace is coloured for what it is, and any frame can be clicked.
- Categories come from the messages themselves, with nothing to declare.
- The list survives a script recompile, which here means C# hot reload.

What is built is described under the editor above. Its lines are kept across hot reload, since the Console is the
editor's own and nothing of it reloads. **Its settings** have a screen of their own, opened from the toolbar's gear,
in the Console's place: a sample of three lines and a stack, drawn by the Console's own row and stack code, then row
height, colour by kind, alternating rows, a category shown as its name, a dot or not at all, the stack's namespaces,
parameters and My code only, a colour for errors, warnings and each part of a frame, how many lines are held, Clear
on play, and the project's category colours. A change shows in the sample and the Console at once.

### VoxDocs

A project's own documentation, in the editor and part of the engine, in its own crate (`crates/voxear_docs`). It
began as the CookieJarTools Cookbook for Unity, rebuilt here and renamed for Voxear; that tool's source and design
are in `Dev/CookieJar` (`Assets/CookieJarTools/Cookbook`, and the guide in `Docs/Cookbook Guide`).

**What exists now** *(first slice)*: a **VoxDocs** tab beside the Scene and Game.
- **Pages are plain Markdown files** in the project's `Docs` folder, so they can be written in any editor and
  shared with git. A folder's own words are in its `_folder.md`. A page's header holds its id, title, tags, when
  it was updated and where it sorts; lines VoxDocs does not know are kept.
- **The Markdown** is CommonMark's everyday parts plus tables, checklists, bare links, `~~strikethrough~~`,
  `==highlights==`, GitHub-style callouts (`> [!NOTE]`, `[!TIP]`, `[!WARNING]` and the rest), heading anchors,
  and `[[Page]]` and `[[Page|words]]` links. It is read by hand, not by a library, into blocks of rich text.
- **Links** go to pages (by id, path or a title only one page has), to headings (`#checklists`, or
  `Page.md#hits` on another page), to paths relative to the page, to files in the project (shown in the Project
  panel), and to the web. A link to a page not written yet is drawn faded and underlined; clicking it makes the
  page beside the one it is in and opens it to write.
- **The panel:** the pages down the side as a tree, a box that finds pages by every word typed, back and
  forward, the page with its title, tags and date, and for a folder what is in it. Ticking a checklist box saves
  the page. Code blocks have a Copy button, and are coloured as a code editor colours them (`highlight`, a small
  reader for each of C#, Rust, TOML, JSON, `.vui` markup and XML, `.vss` and CSS, and shell, for a dark theme or a
  light one, which the editor tells it); their tabs show as spaces to the next of every four columns. The editor's
  Inspector shows a selected script's code, and a UI document's markup, coloured the same way.
- **Tags and headings**, as the Cookbook has them: the tags over the tree, the eight most used and any picked
  (Show all … tags for the rest), each with how many pages have it; picking tags shows only the pages with every one, as
  the find box does with words, and a tag on a page picks it too; past eight tags, a box finds those with the words
  typed in them. **On this page** lists a page's headings down to
  the third level beside it, when it has three or more and the reading area is 640 pixels wide; the one being read
  is picked out as the page scrolls, and a click goes to one.
- **Home**: the top page, under its own words and what is in it, lists the six pages updated most lately with their
  dates, and a line saying how many things need looking after, which opens Docs Health. Any page also opens from the
  command bar (Ctrl+K), which lists every page by its title.
- **Docs Health** *(`health`)*: Health in the toolbar shows, in the page's place, what needs looking after: pages out
  of date (a project file they link to changed on a later day than the page did, by git's last commit of each, or for
  a file with changes not committed, its own time; a page git does not know goes by the date in its header,
  `history`), links that go nowhere (a page not written yet among them), pictures that are not there, and pictures in
  the pages' `images` folders that no page shows. Each entry links to its page or file. Once there is a Code Reference
  it also finds notes written for code that is gone, renamed, moved or taken out, and lists the project's own types
  that no page is about (nothing marked `[Documentation]`) and how many parts of each have no `///` comment, which are
  worth knowing but not counted as things wrong. Git is asked once for every page and file in one look through the
  history, kept until the pages change, and asked again whenever Health is opened.
- **A page's menu** in the tree (a right click) copies a link to it, and for a page of the project's own a
  `[Documentation]` attribute naming it by its id, ready to paste on code.
- **Pictures**: `![what it shows](images/Day.png)`, from the page's folder (pictures kept with a page sit in an
  `images` folder beside it) or the project's. A page shows its PNG, JPEG and GIF pictures at their own size, or
  narrower to fit, keeping their shape; the editor reads them a couple a frame, shrinks any over 2048 pixels, and
  keeps them for the session. Until then, and for one that is not there, its name shows instead. A picture in a
  list or quote shows as its name for now. **Live assets**: a picture naming a prefab, model or material
  (`![The lamp](Prefabs/Lamp.prefab)`) shows its preview, as the Project panel draws it, changing when the asset
  does. A material shows its fields under it too, as the Inspector has them, colour picker and all, to change right
  there; the material is saved and its preview drawn again. A picture file dropped on the editor while a page is
  being written is copied into the `images` folder beside the page (a number added to its name if another is there)
  and put in at the cursor; a file of the project dropped is shown from where it is, a picture or a live asset, or
  linked.
- **Writing a page**, as the Cookbook does: **Edit** shows its title and tags over its Markdown, with the page
  as it will read beside it, drawn again a moment after each change, and the cursor at the end of the text. The
  status says whether it is saved. The page's tags are chips, each with an × to take it off; what is typed becomes
  one at a comma or Enter, and the project's tags that fit what is typed, the most used first, are suggested under
  it, to add with a click. **Save** or Ctrl+S writes it with today's date, and so do **Done** and opening
  another page, so nothing written is lost. A new title renames its file, or its folder, keeping its id, so links
  by id and Back and Forward still find it; tags are kept once each. Under the title, **Image…** lists the project's
  pictures and live assets, **Link…** its pages, and **Action…** the commands and the kinds of check and asset step,
  each putting its Markdown in at the cursor. **Capture Scene** puts in a PNG of the Scene view, and **Record Scene**
  and **Record Game** three seconds of the Scene or Game view as a GIF, fifteen frames a second and no wider than 640
  pixels, made off the main thread. The renderer copies the view before the UI draws over it, so the editor's
  overlays are not in them; when VoxDocs shares its place with the view, the view shows while it is taken and the
  page comes back with the picture. They are kept in the `images` folder beside the page. GIFs play on a page: each
  frame is uploaded once, and the one due swapped in where the picture shows, without drawing the page again.
- **The Code Reference** *(`code`)*: a page for every public type in the game's scripts and in Voxear.Core, in a
  folder after the project's pages, with a folder for each namespace, the game's first. Inside each, the pages sit in
  the folders their sources are in, as `Inspector/Help` for Voxear.Core's `managed/Voxear.Core/Inspector/Help`, read
  from the assembly's debug symbols; a folder the namespace already names is left out. Each time the editor describes
  the scripts, `Voxear.ScriptInfo` writes it beside their build as `Code.json`, from reflection and the XML the
  compiler makes of the `///` comments. A type's page shows how it is declared and what its comment says, then its
  constructors, values, fields, properties, methods and events, each with its declaration, comment, parameters and
  what it returns. An override with no comment shows the comment of what it overrides, so a behaviour's `Update` says
  when it runs. Equals, GetHashCode, ToString and constructors without parameters show only when they have a comment.
  A `<see cref>` to a type with a page links to it. The find box finds the pages, after the project's own (the command
  bar leaves them out, as they would crowd it). A page is made again from the code each time, so what is written on a
  type's page is its **note**: Edit writes it, with tags, and the page shows it under Notes. Notes are kept in
  `Docs/.code-notes`, a Markdown file for each type named by its full name, which the tree does not show; one emptied
  goes, file and all. Any page links to a type as `[[code:Voxear.Prefab]]` or `[[code:Voxear.Prefab|Prefab]]`, and a
  link to a type that is gone shows in Docs Health.
- **Code that points at pages**: `[Documentation("Combat/Damage")]` on a behaviour, component, field, property or
  method names a page by its path or id, with a heading after a hash if wanted. The Inspector shows a book button on
  a documented behaviour's section and beside a documented field, which opens VoxDocs at the page.
  `[Excerpt("Combat/Damage#falloff")]` on a field shows that section of the page above it instead, in a fold that
  starts closed: the section runs to the next heading as big or bigger, and is drawn with the page's own look, again
  whenever the pages change, so writing the page shows in the Inspector at once. A link in it opens VoxDocs. A page
  lists the code pointing at it under **In the code**, each a link to its Code Reference page, and the Code Reference
  says which page documents a type or member, so an attribute naming a page that is not there shows in Docs Health.
  Hello's GameManager has both.
- **Step-by-step guides**: a link to an action or a check on a line of its own is a step. `[Add a light](<action:New
  Point Light>)` is a button that runs the command with that title, as the command bar lists it: the editor's own, or
  a script's `[Command]`, run on the edited level and undone as one step (a title with spaces goes in angle brackets,
  as any address with spaces does). `action:create/Material` makes an asset as the Project panel's New menu does (a
  Folder, Scene, Level, Material or Script), and `<action:create/Material?at=Materials/Brass.material>` makes it just
  there, or finds it there already, showing it in the Project panel. A check ticks itself while what it looks for is
  there, and says under it what is still to do: `check:obj/Sun` for an Obj by name, `check:component/Light` for an Obj
  with a component or a behaviour, `check:file/Prefabs/Lamp.prefab` for a file, and
  `check:code/Hello.Tools.SpinnerAOnTheRight` for a check written in C#: a static method marked `[DocsCheck("…")]`,
  returning `bool`, or `string` with what is still to do. It runs through the scripts (`ScriptRuntime::run_check`)
  against the edited level, once a second; the first starts the scripts, without playing, if nothing has; while
  playing it runs against the game. The editor runs the actions and looks at the checks for the page being read. A
  page with steps says how many are done, with Start again to clear the ticks; the actions each person has run are kept on their
  machine in `.voxear/VoxDocs.toml`. Hello's VoxDocs Guide has one, with a check in Hello's `Tools.cs`.
- `samples/Hello/Docs` shows it: how the sample works, and the guide to writing pages.

- **The Voxear Manual**: the engine's own pages, Markdown in the checkout's `manual/` folder, which
  `voxear-dist` packs as an archive of its own, for the launcher to offer as an optional part of an install, which
  then has them in its `manual/` (`voxear_engine::manual_folder`); an install without it has no manual in VoxDocs.
  The editor shows them in every
  project as a folder after the project's pages and before the Code Reference (`Library::set_manual`). They are read
  like the project's but not written, ticked or looked after: Edit is hidden, Docs Health passes them over, and
  their pictures come from the manual's own folders. Their paths start `manual:`, as `manual:Scripting/Commands.md`;
  a project's page links to one as `[[manual:Commands]]`, by title or path, and `[[Commands]]` in the manual finds
  the manual's own page, as the manual cannot know a project's. They come after the project's pages in a search,
  and are in the command bar and Link…. A guide's steps work in them, so the manual can show a step by running it.
  A test reads the checkout's manual and fails on any link that goes nowhere or picture that is not there.
  The pages are written for people to read: plain words and short sentences, each term explained where it first
  appears, an example before the details, and nothing that sounds generated. They cover getting started, Objs
  and levels, scripting in C# (components and systems among it), models and materials, lighting and the look,
  the game's UI, content and builds, finding problems (the Console, the Profiler, the Frame Debugger, the
  Entities panel and a running game's own tools), and VoxDocs. Where the editor
  cannot do something yet, such as placing a model or setting a scene's sky, a page says so and shows the file to
  write instead.

**Still to come**: the manual's pages on each part of Voxear as it is built: input, audio, physics, animation and
the rest.

**Speed.**
- A **command palette** on Ctrl+K reaches every action, asset and setting *(implemented; see "What exists now")*.
- Keyboard shortcuts appear everywhere, with fuzzy search in every list.
- Selection-aware context actions.
- Every long list (Hierarchy, Project, Console, inspector arrays) is a virtualized list or tree view, so a scene of
  100,000 Objs scrolls as smoothly as one of 10.

**Core.**
- Undo and redo for every edit, as commands *(Inspector and handle edits implemented; see "What exists now")*.
- Play mode with state restore *(implemented; see "What exists now")*.
- **C# hot reload:** game scripts live in a collectible load context. On change, the new build loads, behaviour state
  is carried over, and the old context unloads *(implemented in the editor; see "What exists now")*.

**Extending.** Custom inspectors, windows, gizmos and tools are written in C#, using the same Voxear UI.

### The Inspector and its attributes *(first part implemented in M3)*

**Easy.** Every serialized field shows with a fitting control and no code: numbers, vectors, colours, enums,
`Ref<T>` and Obj pickers, nested `[Serializable]` types as foldouts, and arrays and lists as reorderable,
virtualized list views.

**Attributes, as Odin Inspector has them,** so designers get a tailored inspector from attributes on the field.
They live in `Voxear.Core` beside `[SerializeField]`, so game code needs no editor reference, and cost nothing
at runtime.

```csharp
[FoldoutGroup("Movement")] [Range(0, 20)] [Suffix("m/s")]
[SerializeField] private float speed = 6;

[FoldoutGroup("Movement")] [ShowIf(nameof(canFly))]
[SerializeField] private float liftForce;

[Required] [AssetsOnly] [InfoBox("Plays when the enemy is hit.")]
[SerializeField] private Ref<Prefab> hitEffect;

[Button("Bake path")] private void BakePath() { ... }
```

- **Grouping and layout:** `[FoldoutGroup]`, `[BoxGroup]`, `[TabGroup]`, `[HorizontalGroup]`, `[Title]`,
  `[PropertyOrder]`, `[Indent]`, `[LabelText]`, `[HideLabel]`.
- **Conditions:** `[ShowIf]`, `[HideIf]`, `[EnableIf]`, `[DisableIf]`, `[ReadOnly]`, `[HideInInspector]`, and
  `[ShowInInspector]` for properties and runtime state.
- **Values:** `[Range]`, `[MinValue]`, `[MaxValue]`, `[Suffix]`, `[Multiline]`, `[ProgressBar]`,
  `[ValueDropdown]`, `[EnumToggleButtons]`, `[ColorPalette]`, `[PreviewField]`.
- **Validation:** `[Required]`, `[ValidateInput]`, `[AssetsOnly]`, `[SceneObjectsOnly]`. Problems show on the
  field, and the build checks every scene and prefab, listing failures in the Build window.
- **Help:** `[InfoBox]` (optionally conditional), `[Tooltip]`.
- **Collections:** `[ListDrawerSettings]` (paging, index labels, drag to reorder) and `[TableList]`, which shows a
  list of structs as a table.
- **Actions:** `[Button]` on methods, `[OnValueChanged]`.

Conditions and callbacks name members with `nameof`, so renames carry through and the compiler catches typos.
Voxear.ScriptInfo describes each build of the scripts in a process of its own, so the editor reads no reflection,
and an analyzer makes a wrong name or shape a compile error where the attribute is written (see below).

**What exists now.** The attributes are in `Voxear.Core` (`Inspector/`: Layout, Conditions, Values, Help).
`Voxear.ScriptInfo`, which already describes each build of the scripts in a process of its own, writes what each
field's attributes ask for into the description as an `inspector` object (`InspectorMetadata`), so the editor
still reads no assembly; a source generator can take its place later without the editor noticing
(`script_types::FieldInspector`). The Inspector (`attributes`) applies:
- **Grouping and layout:** `[FoldoutGroup]`, `[BoxGroup]`, `[HorizontalGroup]` and `[TabGroup]`, with "A/B"
  for a group in a group, and the tab picked and the foldouts closed kept while the Inspector builds again;
  `[Title]`, `[PropertyOrder]` (a field's list items and table fields move with it), `[Indent]`, `[LabelText]`
  and `[HideLabel]`.
- **Conditions:** `[ShowIf]`, `[HideIf]`, `[EnableIf]` and `[DisableIf]`, on another field of the same behaviour
  or table, true, non-zero, not empty or set, or equal to a value given (an enum's by its name), read as the
  scene sets them, so they work without the game running; the Inspector builds again when one changes.
  `[ReadOnly]` and `[HideInInspector]`.
- **Values:** `[Range]` (a slider beside the value), `[MinValue]` and `[MaxValue]`, which hold what is typed
  within them, `[Suffix]`, `[Multiline]`, `[ProgressBar]` (a bar a drag sets), `[EnumToggleButtons]` and
  `[PreviewField]` (a larger preview of the asset, under the field).
- **Help and validation:** `[InfoBox]` (a note, warning or error, shown while a field holds if given),
  `[Tooltip]` (under the label once the pointer rests on it for half a second), and `[Required]`, an error above
  an empty reference. `[Documentation]` and `[Excerpt]` link a behaviour or field to VoxDocs (see "VoxDocs").
- **The behaviour's own code, while editing:** `[Button]` on a method that takes nothing puts a button under the
  behaviour's fields; `[OnValueChanged(nameof(Method))]` calls a method each time the field is changed;
  `[ValidateInput(nameof(Method), "message")]` asks a method that takes the value whether it is right (it answers
  `bool`, or `string`, empty when it is) and shows an error above the field while it is not; and
  `[ValueDropdown(nameof(Member))]` offers the values a method, property or field gives, by name when they are
  `ValueDropdownItem`s.
  - In edit mode a behaviour has no instance, so each call runs on a copy made for it: the scene's values set on a
    new instance, bound to its Obj so `Obj` works, with no Awake or other callback, and let go after
    (`InspectorCalls` in Voxear.Core, `ScriptRuntime::inspector`). The scripts start for it without playing, as a
    guide's check starts them.
  - The fields a method changes go back into the scene: a button's as a step of its own to undo, a callback's with
    the change that called it. References a method sets are not carried back yet.
  - Checks and dropdowns are asked again only when the selection or the Obj's scene values change, so a method that
    throws is not asked over and over. An error shows in place, so a field being typed in keeps its focus. A value
    the scene set that a dropdown does not offer stays first in its menu.
  - While playing, buttons and callbacks run on the running behaviour.

- **Collections and colours:** `[TableList]` shows a list of `[Serializable]` values as a table, a column for each
  of their fields and a row for each item, with a button on each row to take it out. `[ListDrawerSettings]` shows a
  list a page at a time (`NumberOfItemsPerPage`), with buttons to turn the page, and labels each item by its index
  (`ShowIndexLabels`) or not at all. `[ColorPalette("#e63946", "#457b9d")]` puts swatches under a `Color` field,
  and a click picks one. `[ColorUsage(hdr: true)]` lets one go past white: the picker adds an Intensity in stops,
  kept as the sRGB colour times 2^stops, and the swatch shows its hue. A list's item is dragged by its label onto another of the same list, which moves it to that
  place, as a step to undo; an item with no label, and a table's rows, are not dragged yet.
- **The build's check:** before its content, a build checks every scene and prefab for a `[Required]` field left
  empty and a value a `[ValidateInput]` method says is wrong, asking the method through the scripts it has just
  started (`field_checks.rs`). Each problem is a warning in the build's log, which the Build window shows, naming
  the file, the Obj and what is wrong, and the report keeps them; the build goes on. A prefab's instances in a
  scene are left to the prefab's own file.

Still to come: `[ShowInInspector]`, `[AssetsOnly]` and `[SceneObjectsOnly]` (a Voxear field takes one or the
other already). A source generator, as
first planned, is not needed: ScriptInfo already gives the editor every attribute without the editor reading the
assembly.

**The analyzer** *(implemented)*, `Voxear.Analyzers`, comes with every game's scripts through
`Voxear.Scripts.props`, so a mistake shows where it is written, in the build and in a code editor:
- `VOX0001`: a condition, or `[InfoBox]`'s `visibleIf`, names no field of its type.
- `VOX0002`: `[OnValueChanged]` names no method of its type that takes nothing.
- `VOX0003`: `[ValidateInput]` names no method that takes the field's value and returns `bool` or `string`.
- `VOX0004`: `[ValueDropdown]` names no method taking nothing, property or field that gives a list.
- `VOX0005`: `[Button]` is on a method that takes parameters.
- `VOX0006` (a warning): `[Range]`, `[MinValue]`, `[MaxValue]` or `[ProgressBar]` is on a field that is not a number.
- `VOX0007`: a `[ColorPalette]` colour is not `#rrggbb`.

Names given with `nameof` are checked by the compiler already; these catch names in quotes and members of the wrong
kind. It is built against Roslyn 4.8, which newer compilers load, and tested by compiling snippets of game code in
memory (`Voxear.Analyzers.Tests`).

**Gizmos** *(implemented)*: a behaviour overrides `DrawGizmos(Gizmo gizmo)`, one callback in place of Unity's two,
and draws with `Draw`, told `gizmo.IsSelected` and `IsHovered`. The editor asks each frame, after debug draw counts
down, for the Scene views that show gizmos (`editor/scene_gizmos.rs`). While editing, a behaviour is only its
scene's values, so Voxear.Core's `SceneGizmos` keeps a copy of each type that draws gizmos (ScriptInfo says which,
`gizmos` on the behaviour), bound to its Obj and filled again only when its values change, and asks it through the
Inspector's JSON calls; while playing, the running behaviours draw theirs. Their lines are marked as gizmos (a bit
in the line call's flags, `Line::gizmo`), and a view draws them only when its `ViewOptions::gizmos` is on: a Scene
view with its Gizmos button on, never the Game view. A gizmo that throws is logged once, not every frame. Voxear.Core
counts the gizmo lines each behaviour draws (`Draw.GizmoLines`) and answers with them by Obj, so the editor keeps each
line with its Obj until the next frame's (`GizmoLines`) and a click within 5 pixels of one selects that Obj ahead of
picking meshes. `[GizmoIcon]` names one of Voxear UI's icons through the `IconName` enum; ScriptInfo writes it as the
type's `icon`, and the Scene views draw it where each Obj with the behaviour is, in a disc a click selects
(`Shape::Icon`, an icon drawn as a shape).

**Handles on fields** *(implemented)*, so a value is dragged where it belongs with no editor code:
`[PositionHandle]` (a `Vector3`, or every point of a list joined as a path), `[RadiusHandle]`, `[DirectionHandle]`,
`[BoundsHandle]`, `[ArcHandle]` and `[SliderHandle]`, each with a `Color`, and points and directions in the Obj's
space unless `Space = HandleSpace.World`. ScriptInfo writes each as a `handle` in the field's inspector settings
(`FieldHandle`), and the editor's `field_handles` draws them for the selected Obj while editing, as dots under the
move, rotate and scale handles: a press on one is taken before the UI, as theirs is, the drag writes the field
through the same path the Inspector uses (`fields::set_raw_at`), and the Obj before the drag and after is one step
to undo. Ctrl snaps points to the snap distance, lengths to tenths, and angles to 5°.

**Handles in code** *(implemented)*, for tools of a game's own: `Handles.Position`, `Radius`, `Slider` and `Button`
in `DrawGizmos`, each taking a value and giving it back as the drag left it. Voxear.Core lists the handles shown
each frame, named by the behaviour's Obj and type and the order it showed them in, and gives the gizmo call's answer
that list; the editor's `script_handles` draws their dots, takes a press on one before the UI, and sends the next
gizmo call which handle is held and where to, or which button was clicked. Only while one of its handles is held
are a behaviour's fields compared before and after its gizmos run, so `DrawGizmos` changing a field on its own never
reaches the scene; the changed fields' `[OnValueChanged]` methods run, and while editing the fields come back to be
written into the scene through `fields::set_raw`, with the Obj before the drag and after as one step to undo. While
playing, the running behaviour simply keeps its new value.

**Custom drawers** *(implemented)* are C#, built with Voxear UI, the UI games draw theirs with: `ValueDrawer<T>`
draws every field of a type, and `AttributeDrawer<TAttribute, T>` (or `AttributeDrawer<TAttribute>` for any type)
the fields marked with an attribute of the game's own. A drawer builds its elements in code in `Build(root)`, once,
or gives its `Markup`, a `.vui` document; and a `Draw(value, ui)` writes the value into the elements and gives it
back as what was clicked leaves it, reading clicks with `WasClicked`. A drawer built in code draws the value alone,
as the Inspector's own control goes only in a markup's `next` element; its elements' events are raised just before
`Draw`. A list's drawers draw each item; a value a drawer draws has no rows of its own.
- **The chain:** ScriptInfo writes each field's drawers by name, outermost first: one for each of its attributes,
  in the order written, then its type's (`drawers`, or `item_drawers` for a list). Each is built in the `next`
  element of the one around it, and the Inspector's own control goes in the innermost one's, or nowhere. The label,
  groups, conditions and decorations stay the Inspector's, so `[InfoBox]` above a drawn field still shows.
- **Running them** (`field_drawers.rs`): a drawn field shows its own control until the edit-mode scripts start.
  Then the editor opens its drawers, which live in `Voxear.Core`'s `InspectorDrawers` until the Inspector is built
  again, and builds their markup, with the project's sheets it names. A field is drawn, innermost drawer first, when
  it is built, when its value changes elsewhere (undo, a callback), and when something in it is clicked or changed;
  all of a frame's fields go in one call through the Inspector's JSON calls, so no interop version changed. While
  they draw, the editor lends the scripts its UI as `ScriptServices` (`DrawerUi`), answering `UiDocument.Find` and
  the element calls through the same `UiNumbers` the game's UI keeps, with numbers of its own from 2²⁴ up. A value
  that comes back changed goes into the scene, typed as the field is, as part of the frame's step to undo, and
  calls the field's `[OnValueChanged]`. A drawer that throws stops drawing until the Inspector is built again.
- While playing, a drawn field shows its own control.
- **The game's own windows** *(implemented)*: an `EditorWindow` in the game's C#, as Unity's, is a dock panel
  opened from any window's menu (Add Level Tools) as many times as wanted, each its own instance, numbered by the
  editor (`window-1`). ScriptInfo lists them (`windows`, with `[Window]`'s title and icon). The editor opens each
  in the scripts, which give its markup, builds that in the panel, and every frame the panel shows has the window
  raise its elements' events and `Update`, through the same JSON calls and `DrawerUi` the drawers use, with
  numbers of its own from 2²⁶ up. It runs in the edit-mode scripts while editing and the game's while playing,
  which are the same runtime, so a window keeps its state across play. One the scripts let go of, as on a
  reload, answers that it is gone and opens again. The editor's memory keeps which window each panel was, so a
  layout opens its windows again.
- **The game's side bar** *(implemented)*, the easy way and the open way: `[SideBarButton(IconName.Grid, "Snap")]`
  on a static method is an icon button on the editor's rail, under Levels, that runs it as a `[Command]` runs
  (`Commands.Run` takes either attribute), its title the button's tooltip; methods sharing a `Menu` share one
  button, which opens a menu of them beside it. A `SideBarItem` is an `EditorWindow` built in a panel of the
  rail rather than of the dock, through the same calls (`open_rail_item`), for custom elements. ScriptInfo lists
  both (`side_bar`: `buttons` and `items`, the items left out of `windows`), and `editor/side_bar.rs` builds them
  again whenever the scripts' description changes.
- **Behaviour inspectors** *(implemented)*: a `BehaviourInspector<T>` draws a whole behaviour, of type `T` or
  derived from it, in place of its fields, as Unity's custom `Editor` does. ScriptInfo names it on the behaviour
  (`inspector`), and it runs as a drawer of a field with no path: the editor opens it with the shown Obj, its value
  is the table of fields the scene sets, and its `Draw(behaviour, ui)` gets a copy of the behaviour made from them
  on its Obj, as `[Button]`s do. The fields the copy ends with changed come back as a table, each set in the scene
  as part of the frame's one step to undo. The behaviour's usual rows are built in a panel after the inspector,
  which moves into its markup's `next` element (`Ui::move_into`), or out of sight without one.

### The design

The editor's visual specification, which the default theme and the editor's sheet implement. It began as the
Graphite + Abyss spec and now takes its colours and density from the editor concept: compact, with narrow gaps.

| Token | Value | Token | Value |
|---|---|---|---|
| bg (window) | `#0C0E10` | text | `#ECEDED` |
| panel | `#191A1C` | text-2 | `#C8CCD1` |
| overlay | `rgba(28,29,33,.92)` | muted | `#9EA4AE` |
| line | `#25272B` | accent | `#7988FF` |
| line-2 | `#2E3036` | accent-soft | `rgba(121,136,255,.2)` |
| field | `#212227` | on-accent | `#FFFFFF` |
| field-line | `#2E3036` | success | `#4CB782` |
| hover | `#232428` | axis X / Y / Z | `#EB5757` / `#4CB782` / `#5E8BF0` |
| selected | `#2D3039` | seg-on | `#393C45` |

- **Type:** Inter 400, 500 and 600 at 12px on a 16px line; titles 12px 600; numbers in JetBrains Mono 11px; small
  labels 11px muted.
- **Shape and density:** panels are rounded 8 with 6px gaps between them; controls are 24px tall and rounded 5;
  rows are 24px, panel headers 32px with 22px tabs, the top bar 40px and the status bar 22px. Icons are 13 to
  14px with a 1.5px round stroke, 16px in the rail; trees indent 14px.
- **The top bar's play controls** sit in a field-coloured pill, play in the accent and pause and step muted, as
  icons rather than filled buttons.
- **Principles:** one accent; thin borders and narrow gaps rather than heavy chrome; shadows only on what floats
  over the viewport; mono for numbers.
- The overlays over the 3D view are frosted glass: `backdrop-filter: blur(16px)` under a lighter wash of the
  overlay colour (see "Backdrop blur" under the UI).
- **Not yet:** the concept's Voxel Brush panel, which is example content.

### The editor concept

The design target for M3's **look and layout**, whose style the design above now defines. The concept's content is only an example scene: its voxel
terrain, brush tools and asset names are placeholders, not planned features.

- **Top bar:**
  - the Voxear mark
  - workspace tabs (World, Animation, Profiling), which became the Layout menu, after Unity's
  - a command bar ("Search or run a command", Ctrl+K)
  - play, pause and step in the centre
  - an accent **Build** button that opens the Build window; the concept's avatar is left out
- **Left rail.** In the concept, icons for the main areas (World, Layers, Code, Audio, Animation), with source
  control at the bottom. The editor's rail opens the Levels and Settings drawers for now; the other areas join it
  as they are built.
- **Hierarchy:**
  - a filter box
  - folders that group Objs
  - a per-type icon on each Obj
  - an accent outline on the selected row
- **Project, with a Packages tab:**
  - a breadcrumb
  - folders with item counts
  - files with their sizes
- **Viewport:**
  - Scene, Game and Shader Graph tabs
  - a shading mode menu, a Gizmos toggle and full screen
  - a floating tool strip: select, move, rotate, scale, tool modes, Pivot/Global and grid snap
  - Unity's axis colours on the gizmo, and a view gizmo bottom right
  - a live stats chip bottom left
  - tools as floating overlay panels with segmented mode buttons, icon grids, swatches, sliders and toggles
- **Inspector, with Lighting and Navigation tabs:**
  - a header with the Obj's icon, name and enabled toggle, then Tag, Layer and Static
  - Transform rows with coloured X/Y/Z fields
  - components as collapsible sections with enable toggles
  - a dashed **Add Component** button at the end
- **Status bar:**
  - on the left, status and script compile time
  - on the right, the graphics API, FPS, memory and the git branch
- **Style:**
  - near-black panels with rounded corners, separated by gaps
  - one accent for selection, toggles and primary buttons (indigo, in the design above)
  - Inter for text, and a monospaced face for numbers
  - compact rows, as in Linear

## Input

Unity's Input System concepts, with the pain points removed and the hard parts built in.

```csharp
// Easy
if (Input.Pressed("Jump")) Jump();
var move = Input.Vector2("Move");

// Advanced
var actions = Input.Map("Gameplay");
actions["Fire"].Performed += OnFire;              // interactions: Tap, Hold, MultiTap, Charge
Input.Players[1].Pair(gamepad);                     // local multiplayer device pairing
await Input.Rebind(actions["Jump"], scheme: ControlScheme.Gamepad);
```

- **Actions, maps and control schemes** are assets. The active scheme switches automatically on the last device
  used.
- **Rebinding is built in:**
  - A ready-made, themeable rebinding screen (`KeyBindingScreen`, built): the keyboard and mouse or a gamepad drawn
    as a key map (`voxear_input::layouts`, `voxear_ui`'s key map, `voxear_engine::keymaps`), each bound control lit
    with its actions labelled round the device, coloured by category, and Rebind beside each action of the key
    clicked. The editor's Input Actions window draws the same map.
  - Conflict detection.
  - Per-player persistence.
  - Composites such as WASD into a Vector2.
- **Prompts are native.** A `<prompt action="Jump"/>` UI element, or `Input.Glyph("Jump")`, shows the correct icon
  for the current scheme, device family and layout (Xbox, PlayStation, Switch, Steam Deck, keyboard). Key names are
  localised and follow the user's keyboard layout, so `KeyQ` shows as "A" on AZERTY. The icon updates instantly
  when the scheme changes. *(Built: a game's own pictures, as Prompt Icons (`crates/voxear_engine/src/prompt_icons.rs`):
  a `.prompticons` file maps each control of each kind of device (keyboard, mouse, Xbox, PlayStation, Switch,
  Steam) to a texture or a part of one. The input actions name the set (`prompt_icons`); the game uploads its
  pictures through `Content::ui_picture` once a renderer is there, and `keymaps::prompt_glyphs_with` gives a glyph
  the picture for its control in place of the drawn one, which `voxear_ui` draws as an image. Under it is Voxear's
  own set, one sheet of a CC0 pack's icons built into the engine (`crates/voxear_engine/prompt_icons/`, credited
  in its `CREDITS.md`), which prompts use when the actions name no set; `builtin:drawn` draws them instead. A key
  the player's keyboard types differently from a US one keeps its drawn keycap, so no picture shows the wrong
  letter.)*
- **Actions are named by id.** Each action in an `.inputactions` file has an `id`, given when the editor makes it
  and to each it finds without one, and kept through a rename; Voxear's own actions have fixed ids. `Actions::find`
  takes an id or a name. An `InputActionRef` field (field kind `action`) stores the id and the Inspector picks it
  from the project's actions; Voxear.Analyzers makes `Actions.Map.Action` from the project's file, or from Voxear's
  own, embedded, when it has none.
- Processors (deadzones, curves, invert), action phases, and polling and event APIs both work. Input is read on the
  main thread and sampled once per frame, plus per fixed step for physics.

## Localization

Every piece of text can be bound to a key. When the language changes, everything bound to it updates at once, with
no reload. This works like Unity's Localization package, but with the setup done for you. *(Built so far: string
tables as `.strings` files, `voxear_localization` finding a key's text through ICU's fallback chains, the system
language on Linux, `Localization.Get`, `Format` with plurals, selects and numbers (dates and currencies to come),
`Language` and `Changed` in C#, UI text
bound with `@key`, `LocalizedString` fields, CSV export and import through `vox loc`, pseudo-localisation from the
palette, and string tables edited in the Inspector.)*

```csharp
// Easy: bind once, and the text follows the language
label.Text = Localized("menu.play");                         // UI
[SerializeField] LocalizedString title = "quest.intro.title"; // fields on Behaviours and Definitions

// Advanced: smart strings, explicit locales, change events
var text = Localization.Format("loot.found", ("player", name), ("count", coins));
// "loot.found" = "{player} found {count, plural, one {# coin} other {# coins}}"
Localization.Language = Locale.Get("pt-BR");
Localization.Changed += OnLanguageChanged;
```

**Tables.**
- **String tables** map keys to text for each locale. Entries can carry a translator comment, a character limit and
  a screenshot of where the text appears.
- **Asset tables** map keys to per-locale assets: voice lines, textures with text in them, and fonts.
- Asset tables go through Content's layered catalogs, so a build only loads the current language's assets, and each
  language gets its own bundles.

**Smart strings.** Messages use ICU MessageFormat:
- variables
- plurals using each language's real plural rules
- gender and select
- per-locale number, date and currency formatting

The formatting runs on ICU4X, the same library behind the UI's text shaping.

**Binding.**
- **UI:** a markup attribute, `<label text="@menu.play"/>`, binds an element to a key.
- **Fields:** a `LocalizedString` field on any Behaviour or Definition.
- **World text:** a `LocalizedText` component.
- **Code:** `Localized(key)` returns an observable value.
- **Prompts:** input prompts and key names are localised through the same system.

**Import and export.**
- **CSV import** from a local file or a **URL**. A Google Sheet published as CSV works directly, so translators can
  keep using a spreadsheet.
- Each import shows a diff (added, changed and removed entries) before applying.
- A URL source can re-sync on demand from the editor or `vox loc pull`.
- **CSV export** for translators, with comments and character limits included. XLIFF and PO follow later.
- **Live remote text** in shipped builds is off by default, and the developer opts in per project.

**Editor window (Localization Tables).**
- A grid with keys as rows and languages as columns, with search and filters.
- Missing or outdated translations are highlighted.
- Every cell is editable inline, with a preview of the plural forms.
- Character-limit warnings.
- Find usages, and renaming a key updates every reference to it.
- A one-click **pseudo-localisation** language, which turns text into accented, expanded text (`[Ƥļåý ŧĥé ĝåɱé !!!]`)
  so layout problems show up before translation starts.

**Language detection.**
- **Order:**
  1. the player's saved choice
  2. the store's language (Steam's game language, when the Steam provider is active)
  3. the OS language: `LANG`/`LC_MESSAGES` on Linux, the user's preferred UI languages on Windows
  4. the project default
- **Fallback chains** handle regional variants, for example `pt-BR`, then `pt`, then `en`.
- The ready-made settings screen includes a language picker.

**Script and layout support.**
- Right-to-left languages mirror UI layout automatically, and BiDi text shapes correctly (parley).
- CJK, Arabic and other scripts get per-locale font fallback chains.
- Missing keys show as `⟦menu.play⟧` in development builds and log a warning. Release builds fall back silently
  along the fallback chain.

## Audio

```csharp
Audio.Play(Sounds.Explosion, at: obj.Position);   // easy: 3D one-shot with sensible falloff
```

**Mixer.** A Rust mixer on cpal (`voxear_audio`), on a thread of its own, with buses, effects, snapshots and ducking.
2D and 3D sources, Doppler, and streaming for music. *(Built so far: music and effects buses under a master volume, and
the editor's own preview voices outside them; 2D voices, and 3D ones that fade by the inverse of distance down to
silence at a max distance and pan with equal power from the listener; voices that loop, pause, and change volume and
pitch without clicks, resampled to the device's rate; and long sounds decoded a block ahead on a streaming thread.
The output follows the system's default device: when that device goes away or another becomes the default, as when
headphones are unplugged, the output thread opens the default again, and with none it mixes quietly until one turns
up; Doppler on 3D voices, from each source's and the listener's speed frame to frame, with a level per source; and
HRTF for headphones from Steam Audio; each bus mixed on its own, with a Butterworth low-pass and high-pass, the music
ducked under the effects by an envelope follower, and the volumes and cutoffs sliding to a snapshot's over the time
`Audio::set_transition` asks for.)*

**Sounds** are assets: WAV, OGG Vorbis, FLAC and MP3, decoded by Symphonia. Their `.meta` says whether one loads whole
or streams (by default by its length) and its volume. An **Audio Source** component plays one from its Obj, and the
listener is the first Obj with an **Audio Listener**, or else the game's camera. Each frame, after the game's update,
`GameAudio` brings every source's voice in line with its component and position. Scripts get `Audio.Play(sound)` and
`Audio.Play(sound, at:)` for one-shots, `Obj.AudioSource`, and the bus volumes. A build packs the sounds that scenes and
script fields use, each with its `.meta`, and compressed files as they are. A background load reads and decodes the
sounds its scenes and prefabs use on the loading thread, so they come in with no IO. In the editor, a sound dragged
from the Project into a Scene view or the Hierarchy makes an Obj named for it with an Audio Source playing it.

**Spatial acoustics** come from Steam Audio:
- HRTF binaural rendering. *(Built: Steam Audio 4.8.1 is loaded with `libloading` when first wanted, from
  `VOXEAR_STEAM_AUDIO`, beside the program, or `crates/voxear_audio/steam-audio/lib`, which `fetch.sh` fills from
  Valve's release, so Voxear builds and plays without it. With `Audio::set_headphones`, the mixer makes the HRTF at the
  output's rate on a thread of its own, then gives each 3D voice a binaural effect: the voice's mono samples, at its
  distance's volume, go in blocks of 256, and its stereo plays a block behind, playing out when the voice ends. Without
  the library, or before the HRTF is ready, 3D voices pan. Player builds copy the library beside the player with its
  licence, unless `[audio] steam_audio = false`.)*
- **Occlusion and transmission:** sound is muffled through walls, and materials set absorption. *(Built: from the
  first Audio Source with occlusion on, `GameAudio` keeps Steam Audio's scene of the level's still colliders, those
  without a Rigidbody and not triggers, each as triangles of its `Collider.acoustic` material, built again when one
  changes; each frame Steam Audio's direct simulation, raycast or volumetric, gives each such source a `Muffle`, a
  volume in each of three bands, and the mixer splits the voice there with two one-pole filters. Moving colliders,
  as Steam Audio's instanced meshes, come later.)*
- **Reflections and reverb:** real-time or baked reverb probes, with echo from real geometry. *(Built so far:
  listener-centric reverb. With `Audio.Reverb`, a `RoomReverb` thread keeps its own Steam Audio scene of the same
  walls and runs parametric reflections from the listener every 100 ms at most, 4096 rays of 16 bounces; the RT60 in
  each band it gives drives the mixer's own Freeverb-style stereo reverb, eight combs and four all-passes a side,
  which the 3D voices on the effects bus are sent to in mono. With `Audio.ReverbMode` or `[audio] reverb_mode` set to
  convolution, the thread runs convolution reflections instead, at the mixer's rate (`Audio::sample_rate`, restarted
  when the device's changes), and hands the mixer once a `steam_audio::Convolution`: its own HRTF, a reflection
  effect and an Ambisonics decode effect, made off the audio thread, and a retained reference to the room's source.
  Each block of 256 the mixer reads the source's latest impulse response with `iplSourceGetOutputs`, which Steam
  Audio triple-buffers between the simulating thread and the one reading, so only the audio thread reads it; plays
  the send through it into first-order Ambisonics, two seconds long; and decodes that to stereo, or binaurally with
  headphones on, as the listener faces. The convolution takes Freeverb's place, which fades as it comes in, and
  fades itself out when reverb goes off. Echoes that will not start fall back to reverb times. Baked probes come
  later.)*
- **Pathing:** sound bends around corners and through doorways.

**Ray-traced audio (optional setting).** A per-project and per-quality-tier switch traces sound on the GPU instead
of Steam Audio's CPU ray tracer, through Steam Audio's custom scene callbacks in batches:
- Rays run against the same acceleration structures as ray-traced GI and the lightmapper, with hardware ray queries
  where available and the compute BVH elsewhere. Nothing extra has to be built or kept in sync.
- Thousands of rays per source give real-time reflections, echo and reverb from the actual geometry. Opening a door
  or collapsing a wall changes how a room sounds immediately, with no baked probes.
- Each surface's acoustic material (absorption, scattering, transmission per frequency band) comes from its render
  material, with an override.
- When the switch is off, or the GPU cannot spare the time, the same sources fall back to baked reverb probes and
  ray-cast occlusion, so games never depend on it.

**Tools.** An audio debugger shows active voices, occlusion rays and paths in the viewport. *(Built so far: Audio
Sources as speakers in the Scene view, lit while they play, and the Audio panel, which lists every voice the mixer
plays, named by `GameAudio::heard_voices`, from a list the mixer writes after each block only while the panel watches,
and never waits for.)*

## Physics (Jolt)

Rigid bodies, colliders (including convex and mesh), joints and a character controller. Queries cover raycasts,
shape casts and overlaps, and are callable from jobs. Collision and trigger events arrive as ECS events and as
`OnCollision`/`OnTrigger` behaviour callbacks. Jolt runs on the engine's job system. The physics step runs in the
`FixedUpdate` stage and is replaceable like any system.

## Animation

**Skinning.** Skinned meshes are skinned in compute shaders, so the GPU does it once per frame for every consumer.
Blend shapes are supported.

**State machines.**
- Layers, avatar masks and blend trees (1D, 2D and direct).
- Sub-state machines and transitions with conditions.
- Root motion and animation events.

**Motion matching**, the "better system":
- An offline **feature database** stores, per pose, foot positions and velocities, hip velocity, and the future
  trajectory at three sample times.
- Features are normalised per channel, with designer-tunable weights.
- At runtime, the search runs every N frames. It uses a SIMD brute-force or KD search on the CPU for a few
  characters, and moves to the **GPU for crowds**.
- **Inertialisation** blends between poses with no pose buffers.
- Trajectory prediction uses critically damped springs.
- Tags (crouch, combat) filter the database.
- The easy path: drop in a locomotion clip set, pick a preset, and it works.

**IK and procedural.** Two-bone IK, FABRIK, look-at, foot placement on terrain, and procedural layers.

## Particles

Research covered Shuriken, VFX Graph, Niagara (including its lightweight emitters), Godot, PopcornFX, EmberGen,
Effekseer and Houdini. The pattern is clear:
- **Stacks with presets are beginner-friendly.**
- **Raw graphs with simulation contexts are hostile to beginners.**
- The best idea to copy is Niagara's *lightweight emitters*: simple by construction, and upgradeable.

**One intermediate representation, three skins:**

1. **Core.** A particle asset is an ordered list of typed modules in fixed stages: Spawn, Init, Update, Render, and
   optional custom simulation stages. It compiles to one GPU program. The GPU side is:
   - a spawn counter
   - alive and free lists
   - indirect dispatch and indirect draw
   - sorting only for alpha-blended systems
   - **A stateless fast path:** if every module is a closed-form function of age, seed and parameters, there are
     no state buffers and evaluation happens in the vertex shader. Adding collision or a ribbon promotes the system
     automatically. Users never choose between the two.
2. **Easy.**
   - A **preset gallery** with live thumbnails: fire, smoke, sparks, explosion, magic, impact, dust, rain, snow,
     leaves.
   - Each preset shows about 8 *look* sliders (size, energy, colour, lifetime, wind, softness, lit, quality) and
     scrubbable curves and gradients.
   - Soft, lit particles are on by default, and "collide with world" is one checkbox.
   - Presets are real stacks, so nothing is hidden.
3. **Stack.** "Show modules" reveals and edits the module stack, Shuriken style.
4. **Graph.** Any module opens as a node graph or as Slang code, scratch-pad style. Data interfaces cover SDF,
   mesh surface, skeleton and audio spectrum.

**GPU features.**
- Events and sub-emitters through append buffers, with no CPU readback.
- Ribbons and trails.
- Mesh particles.
- Collision against the depth buffer first, then the global SDF from voxel GI.
- Attractors and vector fields.

**Content workflow.**
- One-click **bake to flipbook**.
- Import of EmberGen and Houdini flipbooks, including 6-way lit smoke.

**Scalability and debugging.**
- A budget class per system, with distance and screen-size LOD and a global particle budget.
- A debugger showing alive count against budget and per-emitter GPU time.
- Click-to-inspect particles, pause, step and seed lock.
- A "why can't I see it?" checker covering bounds, culling, alpha and depth.

## Terrain

Three kinds of terrain, each a component of its own under the Terrain module: **Terrain** (heights), **Block
World** (blocky voxels, from big blocks down to small coloured ones) and **Voxel Terrain** (smooth, flat or a
planet). They share generation (`.worldgen`), ground surfaces (materials laid by rules and paint) and plants where
those fit.

**Where the work runs.** Gameplay data is worked out on the CPU, the same on every machine: heights, voxels,
generation, collision, saves and the edits sent over the network. A dedicated server has no GPU, and physics reads
the same data the game shows. Drawing, culling, erosion simulation, grass and tracks run on the GPU.

**Heights.** `voxear_terrain` holds them in tiles of 256 quads, 257 heights a side, each sharing its edge with the
next, held as `Arc`s: a change makes new tiles and keeps the rest, and the renderer and physics see which changed by
pointer. Between heights the ground is two triangles a quad, split from a quad's low corner to its high one, as Jolt
splits height fields, so what is drawn, what collides and what a ray hits agree. A `.terrain` file says how large
and how fine the ground is; its heights are kept beside it in a `.heights` file (each tile's samples split into byte
planes, then LZ4), with its holes, a bit a sample, and its surfaces' shares, so a game reads them as made. A hole
cuts away every triangle with a corner on its sample, in drawing, in rays and in Jolt (whose no-collision height
cuts the same triangles).

**Layer stack.** A terrain is a stack of layers, each a *command* rather than a raster: base, import, stamp
(picture or mesh), noise, sculpt raster, spline (roads, rivers, trenches), terrace, smooth, erosion and generate,
each with a blend op (add, subtract, max, min, set, blend, multiply), opacity, a mask by height and slope, and
sizes in metres. Compositing runs on the CPU across worker threads, over the dirty tiles alone: each layer's
contribution is cached by a hash of what changes it, with checkpoints every few layers, so editing layer *k*
replays only *k* to *n*. Derived per tile: normals, slope, curvature, flow, lowest and highest heights for culling
and rays, and a basemap of average colour.

**Erosion.** A world generator's gullies, per point (stripes down the slope octave by octave, each following the
last), suit endless chunked worlds. The Erosion layer wears finished ground on the GPU once, in the editor
(`shaders/erosion.slang`): pipe-model water with sediment passed on with each pipe's share of the water (mass
kept), then slumping past an angle gathered from eight neighbours; read back and kept as the layer's raster, so
games read it as made. To come: stream-power erosion for mountain ranges, and flow, sediment and debris masks for
texturing and scattering.

**Generation.** A `.worldgen` asset (`voxear_terrain::generate::WorldGen`) makes ground at any point from a seed:
three fields read through one warp (continents, ruggedness, and ridged noise from valleys to crests), shaped by
four curves a designer draws, smoothed through their keys (coast and inland over continents, relief over
ruggedness, peaks over ridges: height = coast + inland × relief × peaks), with crags and bumps, from presets. Only
sums, products, square roots and `floor`, so the same seed makes the same bits on every machine. A terrain's
Generate layer lays it from an offset in its world; compositing finds generators by reference as it finds
pictures. Rivers and lakes on finished maps are a Rivers layer (`voxear_terrain::rivers`): priority-flood filling
gives every cell the way it drains and the ground draining through it; rivers are cut where that is enough, lakes
stand in the filled hollows, and the editor makes Streams and Pools for them. A generator's climate (temperature
and humidity fields, cooling with height) feeds surfaces' rules, so biomes choose surfaces and the grass on them.
A terrain without end (`voxear_terrain::endless`) keeps a window of tiles round the camera, each made alone on
background threads from the layers worked out from where each sample is, with its shares and grass from its padded
ground; renderer and physics keep each tile in a slot by where it is in the world, so a moving window copies in
only the tiles that came into it. To come: rivers along valleys in endless worlds; biomes choosing trees and
blocks. For voxels, 3D density from the same fields, with caves.

**Rendering.** CDLOD: a quadtree of patches, each a grid of 32 quads drawn from one index buffer, picked on the CPU
for the camera and each sun cascade, each vertex sliding onto the next level's grid near its level's far distance.
Heights live on the GPU in a buffer of tile slots that only changed tiles are copied into. The ground is shaded by
the forward passes' own fragment shaders, drawn early so hills hide what is behind them; a coarse mesh a tile,
which only rays see, gives the GI and the sea floor the ground. Surfaces: up to 32 materials, each read through the
bindless table, so no texture arrays; each surface covers those below it by rules of height and slope with wandering
edges, and by what was painted (a raster a surface), and each sample keeps its four largest shares (four surface
numbers and four shares out of 255, two words) in the terrain's GPU buffer beside its heights and holes. The fragment
shader blends the four samples round a pixel, then by the surfaces' height pictures, sampling each with gradients
worked out before any branch; triplanar where a surface asks. Distance resampling, variants and hex tiling are to
come. GPU culling of patches against last frame's depth, and horizon culling over the heights, come with the chunk
culling below.

**Plants.** Placed plants (trees, rocks) are a Foliage's `Plants` (`voxear_scene`), numbered by their place, kept
with or without a renderer, so trunks collide on servers too; one cut down is only marked gone, so nothing is built
again and saves name it by its number. Grass and details are densities a byte a kind a sample, scattered on the GPU
each frame round each view's camera on a grid fixed to the world, into runs of blades per kind and ring of
distance, drawn by indirect draws outside the scene's instances (`grass.rs`, `shaders/grass.slang`).

**Tracks.** Only drawn, so on the GPU: each view keeps a map of the ground round its camera (1024 texels of 6 cm a
side), holding the world's cells modulo its size so it scrolls without copying; a compute pass clears what came
into it and presses each Track Maker's path since last frame where it touches the ground (`tracks.rs`,
`shaders/tracks.slang`). Each texel keeps its deepest press and when; surfaces say how far they give and how soon
they fill in. The ground's pixels march the eye's ray to the tracks' floor and bend their normal by its slope.
A surface's liquid (wet, puddles, lava) and glitter are flags it carries to the shaders, so they go where it covers:
puddles stand where the ground lies below the lowest of a ring of heights round it, and lava's crust rides down the
ground's fall. A model's material can blend its foot into the ground: the forward shader finds the terrain under
the pixel and samples its surfaces there, as the terrain does.

**Voxels.** Chunks of 32³, in one GPU geometry pool filled through an asynchronous upload ring, culled on the GPU
against the frustum and last frame's depth and drawn by multi-draw-indirect, with compact vertices of their own.
Block worlds (`voxear_terrain::blocks`): chunks of 32³ blocks kept sparse by where they are, each a palette of the
blocks in it and indices in as few bits as it needs; faces where a block meets air, shaded at their corners by the
blocks closing them in and merged into quads by surface and shading (greedy meshing), two words a quad, kept by the
way they face. A world's quads live in a pool of its own and are drawn six vertices a quad with no vertex buffer
(`blocks.rs`, `shaders/blocks.slang`), each view drawing only the chunks in view and their faces turned towards it;
each chunk's faces merged by surface alone are its collision mesh and its mesh for the GI's rays. A change remakes
the chunks it reaches and those beside it it shades, on every core; changes are kept as what differs from what a
world was made as (`blocks::edits`), in a `.blocks` file for the editor's and in a scene's save for scripts'.
Generated block ground (`blocks::generate`) takes a world generator's heights a column at a time, pushed in and out
by 3D noise and hollowed by caves, its top covered by kinds of block by climate and height. Generated ground without
end streams chunk columns round the camera (`blocks::stream`), the world's own near, and beyond them rings of
coarser chunks, each twice the last's block size, kept by level in the renderer's pool. Models of small coloured
blocks come from `.vox` files (`blocks::vox`), each colour built as a kind of block. A world drawn smooth (a Voxel
Terrain) keeps how full each block is near its surface and is meshed by surface nets (`blocks::smooth`), drawn from
the same pool as triangles' corners. A planet's ground lies all round its world's corner, its generator read round
it in three dimensions on a cube's faces once; physics pulls bodies near it towards its middle. Each chunk of cubes
notes which of its sides see each other through its air, and each view draws only the chunks a walk from the
camera's chunk through those reaches (`blocks::sight`). Far out, the world's middle follows the camera (see the
floating origin, under the Obj model), and chunk keys count from the world's Obj, so none is made again.
Digging into a
heightmap terrain turns its columns into smooth-voxel chunks that start from its heights (`voxear_terrain::dig`,
two voxels a sample), holes the heights in them, and meshes them by marching tetrahedra, which meet the heights
exactly along the columns' edges. Dug chunks' triangles are made on the CPU when they change and drawn from a pool
of the terrain's own by its pipelines (`dug.rs`); physics has a mesh body a chunk. A chunk keeps its triangles once
made, at three levels of detail (its points gathered into cells of two and four voxels, those on its faces kept so
levels meet), made on the terrain's background threads (`voxear_terrain::workers`): drawing and physics keep the chunk
as it was, and a tile holed for new dug columns as it was, until they are done. Rays see dug ground through the
tiles' meshes for rays, which carry their dug chunks at their coarsest. In the layer stack, dig strokes
and trenches (a cross-section swept along a spline) are layers that lay no heights: they are dug after compositing,
in order, as a terrain loads and again in the editor only when they or the heights under their columns change, so
saved heights keep none of their holes (`dig::redig`).

**Runtime editing.** A script's edit (raise, crater, flatten, paint, holes, grass: `voxear_terrain::runtime`) is a
small command applied straight to the tiles it reaches, the layer stack left alone, with nothing but sums, products,
square roots and smoothsteps, and heights set to whole steps of 1/1024 m: the same edits make the same ground on
every machine, so a networked game sends the edit rather than the ground. Physics and drawing follow the new tiles
by pointer, as they follow the editor's. Each edited terrain keeps the ground as built and its edits since
(`TerrainEdits`); a save of the scene, or a player joining, is given whichever is smaller, the edits one by one
(about 20 bytes each) or each changed tile as how it differs from the one built (heights as steps from what their
neighbours foretell, shares and grass XORed, split into byte planes, then zstd). Still to come: choosing the form a
tile at a time, voxels dug in the same record, LZ4 for streaming from disk, and joiners sent the nearest first.

```csharp
Terrain.HeightAt(position);                                   // asking
Terrain.Crater(hit.Point, radius: 3f, depth: 1.2f, Dirt);     // easy
Terrain.Apply(TerrainEdit.Flatten(at, 4f, floor));            // advanced
var edits = Terrain.CaptureEdits();                           // for a save, or a player joining
```

## Navigation

Modelled on Aron Granberg's **A\* Pathfinding Project**, not Unity's NavMesh.

**Graph types.**
- Grid and layered grid (multi-floor).
- Recast navmesh, tiled and updatable at runtime.
- Point graphs.
- Several graphs coexist, for example one per agent size.

**Pathfinding.**
- Path requests are async and multithreaded on the job system, with a synchronous option.
- Path types: ABPath, MultiTarget, Random and Flee.
- Tags and penalties allow weighted terrain such as "prefer roads".
- **Graph updates at runtime:** obstacles cut or update only the affected tiles or nodes.

**Movement.**
- **Modifiers** post-process paths: funnel, smoothing, raycast simplification, and start/end snapping.
- **Local avoidance** uses RVO with thousands of agents, SIMD and burst-style data layout.
- The easy API is `agent.Destination = target;`, with sensible movement and avoidance defaults.

## Cameras and Timeline

Cinemachine's capability with an API people can actually read.

```csharp
// Easy: one line per common camera
Cameras.Follow(player).Orbit(distance: 6f).AvoidWalls();
Cameras.Cut(to: cutsceneShot);  Cameras.Blend(to: aimShot, seconds: 0.3f);

// Advanced: compose behaviours on a rig
rig.Position = new Orbit(target, distance: 6f) { Damping = 0.15f };
rig.Aim = new Composer(target) { DeadZone = new(0.1f, 0.1f) };
rig.Add(new Shake(Shakes.Handheld), new Collision { Radius = 0.3f });
```

**Rig presets.**
- One **Camera Rig** component with presets: Follow, Orbit, Third Person, First Person, Top Down, Rail/Dolly,
  Look At, Group Framing and Fixed.
- Each preset shows only the handful of properties that matter, with live gizmos in the viewport.
- Priorities, blending, impulses (shake) and collision are built in.

**Timeline.**
- Tracks and clips: animation, activation, audio, camera shots, particles, signals and custom C# tracks.
- Scrubbing works in edit mode, and timelines can nest.

## Networking

The model is server-authoritative and ECS-native.

**Easy API.**
- `[Replicated]` on component fields.
- `[ServerRpc]` and `[ClientRpc]` methods.
- Spawning a networked prefab replicates it.

**Advanced API.**
- Relevancy and interest management.
- Prediction with rollback for owned entities, and interpolation for the rest.
- Lag compensation.
- Custom serialisers, with bit-packed delta compression against acknowledged snapshots.

**Transport and testing.**
- Transport is UDP plus QUIC (quinn), with relay and session/lobby interfaces.
- A network simulator (latency, loss, jitter) is built into play mode.
- Play mode can start multiple instances.

## Modding

Modding is part of Content and scripting rather than a separate loader. A game turns it on with one setting, and
the developer decides exactly what mods may do.

### What a mod is

A mod is a `.voxmod` package: an archive holding built content, optional script assemblies, and a `mod.toml`
manifest.

```toml
[mod]
id = "ana.better-swords"        # globally unique
name = "Better Swords"
version = "1.4.2"               # semver
game = ">=1.2, <2.0"            # game versions it supports
api = 1                         # Mod API major version it was built against

[dependencies]
"ana.weapon-lib" = ">=2.1"

[load]
after = ["sam.balance-tweaks"]

[permissions]                   # shown to the player before enabling
scripts = "sandboxed"           # "none", "sandboxed" or "full"
```

### Three tiers of trust

| Tier | What it can do | How it is kept safe | Default |
|---|---|---|---|
| **Content** | Add or override textures, models, audio, levels, UI, localisation and Definitions; patch Definition fields | There is no code. Everything is parsed by memory-safe Rust decoders with size limits. | On whenever modding is on |
| **Sandboxed scripts** | Add Behaviours, systems and event handlers in C# | Every referenced type and member is checked against an allowlist at load. Each mod gets its own load context, and faults are isolated. | Developer opts in |
| **Full trust** | Anything, including Harmony-style method patching | Nothing: the player sees a clear warning and approves each such mod. | Off |

### Content mods

**Layered catalogs.** Content catalogs stack: the base game first, then each mod in load order.
- A mod can **add** assets (new GUIDs).
- A mod can **override** existing assets by GUID, but only those the game marks as overridable (by folder, type
  or asset).
- `Ref<T>` does not change, so game code never knows whether a texture came from a mod.

**Definition patches.** Mods can change individual Definition fields without replacing the asset, for example
"swords deal 10% more damage". Two mods patching the same field show up as a conflict in the mod manager.

**Building.** Mods are built by the same content build, so they are bundled and optimised the same way.

### Script mods

**The Mod API.**
- Mods compile against the game's **Mod API**: the types and members the game marks `[ModApi]`.
- They also see Voxear's own mod-safe API and an allowlisted part of the .NET base library (collections, maths,
  strings, LINQ, engine-scheduled tasks).

**The access check.** At load, each mod assembly is scanned for every type and member it references, the approach
s&box uses.
- Anything outside the allowlist rejects the mod with a readable reason, such as "uses `System.IO.File.Delete`,
  which mods may not use".
- File, network and process access are blocked, as are reflection, P/Invoke, `unsafe` code and raw threads.
- The same scan catches **mods broken by a game update** before they run, such as "uses `Weapon.Damage`, which was
  removed in 1.4", instead of crashing mid-game.

**Isolation and storage.**
- Each mod loads into its own collectible context. It can be unloaded and hot-reloaded during mod development, and
  it only sees other mods it declares as dependencies.
- Exceptions are caught per mod. A mod that keeps throwing, or overruns its frame budget, is disabled with a message
  to the player instead of taking the game down.
- In-process .NET cannot safely interrupt an infinite loop. The watchdog therefore flags the mod and starts the next
  launch with it disabled.
- Mods get `ModStorage` (a private folder) and a settings API instead of raw file access.

**Later, a WebAssembly tier** for hard isolation:
- Mods compile to WebAssembly components and run under wasmtime with memory limits and epoch interruption, so even
  an infinite loop is stopped.
- Rust and C mods work today. C# would come through componentize-dotnet, which is still in preview.

### Versions, dependencies and load order

- A mod declares the game versions it supports and the Mod API major version it was built against. The Mod API
  follows semver.
- **The developer chooses the policy for outdated mods:** `allow`, `warn` (the player decides), or `block`. A
  separate choice covers whether mods built for an older Mod API major version may load at all.
- Dependencies resolve with version ranges. Load order comes from dependencies plus `before`/`after` hints, sorted
  topologically.
- Cycles, missing dependencies and conflicts are listed in the mod manager with a fix where one exists.
- Saves record which mods and versions were active. Loading a save without them warns first.

### Security features

- **Hardened parsing.** Content is decoded only by memory-safe Rust code, with limits on texture size, triangle
  counts, audio length and archive size. Path traversal ("zip slip") is rejected, and no executable content exists
  outside the script tiers.
- **Signing.** Mods can be signed with Ed25519. A game can require signatures from keys it trusts, for example in
  curated or competitive modes, and show verified authors.
- **Consent.** The player sees the tiers a mod asks for before enabling it. An update that asks for more re-prompts.
- **Multiplayer.** The server publishes its mod set (IDs, versions, content hashes) and clients must match it.
  Missing mods can download automatically from the configured source. The server can forbid client-only mods.
- **Safe mode.** `--no-mods`, and an automatic offer to start without mods after a crash during mod loading.

### Developer control

**Easy.** In Project Settings, open **Modding**, tick **Enable modding**, and pick sources (local folder, Steam
Workshop, mod.io). The game gets a ready-made, themeable mod manager screen with `Mods.ShowManager()`.

**Advanced.**
- The allowed tiers.
- Which assets are overridable and which Definitions are patchable.
- The `[ModApi]` surface.
- The outdated-mod policy and signature requirements.
- Per-mode rules, such as no script mods in ranked play.
- `Mods.Loaded` and `Mods.OnLoaded`.
- `[ModEvent]` hooks.
- Custom mod sources, and control of load order from code.

```csharp
// Game: expose data and an extension point to mods
[ModApi] public sealed class WeaponDefinition : Definition { public float Damage; public Ref<Mesh> Model; }
[ModApi, ModEvent] public static event Action<Obj> EnemySpawned;

// Mod: plain Voxear code, checked against the allowlist when it loads
public sealed class GoldenEnemies : ModEntry
{
	protected override void OnLoad() => Game.EnemySpawned += enemy => enemy.Get<Tint>().Colour = Colours.Gold;
}
```

### Tools for modders

- `vox mod sdk` exports a **Mod SDK** from the game: reference assemblies for the Mod API, Definition schemas and
  templates.
- Modders use the Voxear editor in **mod mode**: base game content is visible but read-only, and only their mod is
  editable.
- The CLI covers the same work: `vox mod new`, `vox mod build`, `vox mod test`.
- `vox mod publish --workshop` uploads to Steam Workshop with a preview image, tags and change notes. The editor has
  the same button.

## Platform services (Steam and other stores)

Store features go through one `Platform` API. Steam is built in, and other stores are providers behind the same
interface.

**Services.**
- Identity: user ID, name and avatar.
- Achievements, stats and leaderboards.
- Cloud saves.
- User-generated content (Workshop).
- Rich presence, friends and invites.
- Lobbies and matchmaking, and networking relay.
- Entitlements and DLC.
- The overlay, screenshots and timeline markers.

**Capability checks.** Each provider reports what it supports, through `Platform.Supports(Feature.Workshop)`.
Missing features degrade cleanly; for example, achievements are tracked locally when offline.

**Defined once.** Achievements, stats and leaderboards are Definitions, with a localised name, an icon and an ID
mapping per store. The same call works on every store:

```csharp
Platform.Achievements.Unlock(Achievements.FirstBlood);
```

**Providers.**
- **Steam**, built in.
- **Offline**, for development and DRM-free builds.
- Planned: Epic Online Services, GOG Galaxy, mod.io (mods across stores), and itch.io (uploads through butler).
- A custom provider implements the provider interface, in Rust or C#.

**Steam out of the box.**
- **Setup.** Steamworks is loaded at runtime through its flat C API. If Steam is not running, the game falls back to
  the offline provider, unless the developer requires Steam. The App ID lives in project settings, and
  `steam_appid.txt` is handled automatically during development. Callbacks are pumped in the `First` stage and
  arrive as C# events or awaitable tasks.
- **Workshop.**
  - Subscribed items appear in the mod manager as a mod source.
  - Updates are detected and installed.
  - Publishing works from the editor and the CLI.
- **Steam Input.**
  - Input actions map to Steam Input action sets automatically.
  - Prompts use Steam's glyphs for any controller.
  - Steam Deck is detected, for UI scale, the on-screen keyboard and suspend/resume.
- **Networking.** Steam Networking Sockets and Steam Datagram Relay serve as a transport for the multiplayer layer.
  Lobbies and invites work through the overlay.
- **Other services.** Steam Cloud saves, rich presence, the overlay, screenshots and Steam Timeline markers.
- **Shipping.** `vox steam upload` pushes a build through SteamPipe, generating the depot configuration from the
  project.

## 2D

- Sprites and automatic atlases.
- 2D animation and sprite skinning.
- Tilemaps with rule tiles and auto-tiling, plus isometric and hex grids.
- Box2D v3 physics.
- 2D lights and shadows.
- A pixel-perfect camera.
- 2D shares the renderer, ECS and content pipeline with 3D, so mixing the two is natural.

## CLI (`vox`)

Every operation the editor can do should be scriptable.

**Now.**
- `vox doctor` checks the GPU and .NET, and in a source checkout the shader compiler, which an install does not
  need.
- `vox run <project> [--headless] [--frames N]` runs a project.
- `vox render <project> [--post <preset>] [--debug-view <view>]` renders a frame offscreen to a PNG and prints GPU
  timings, culling stats and the adapted exposure.
- `vox editor <project> [--screenshot <png>]` opens the editor, or renders it offscreen.
- `vox content build <project>` packs the content into bundles and prints each bundle, its usage set and its assets,
  then what was left out.
- `--bundles` on `run` and `render` reads assets from those bundles, as a player will, and builds them first unless
  `--no-build`.
- `--stats` on `run` and `render` shows the stats overlay (F1 toggles it in a window).

**Planned.**
- Project: `vox new`, `vox open`.
- Building: `vox build [--profile <name>] [--progress json]`, `vox scripts build`.
- Testing: `vox test`.
- Scenes: `vox scene list/inspect`.
- `vox import`, `vox profile`.

**Editor RPC.** A running editor exposes a local JSON-RPC socket, so `vox` can drive it: select, create and modify
Objs, run play mode, capture the viewport. AI tooling can later sit on the same RPC, for example as an MCP server,
without changing the engine.

## Debugging and profiling tools

The debugging and profiling tools are built in and on in the editor and development builds. `vox` can also reach
them in a running game.

**Easy.**
- One key toggles a **stats overlay**: FPS, a frame-time graph, the CPU/GPU split, draw and triangle counts, and
  memory. *(First version implemented, in markup: F1 in a game window, or `--stats` on `vox run` and `vox render`; clicking its header folds it. It shows
  frame rate, frame time, GPU time, draws, shadow draws, culled instances, lights per cluster and exposure. The
  graph, triangles and memory come with the profiler.)*
- The **capture button** in the toolbar grabs the next frame in RenderDoc.
- Every panel below opens from the command palette.

**Advanced.**

- **GPU frame capture.**
  - RenderDoc is integrated through its in-app API. When it is installed, the engine loads it at startup (or use
    `vox run --renderdoc`), and a hotkey or toolbar button captures a frame and opens it.
  - The render graph names every pass, resource and draw through `VK_EXT_debug_utils`, so a capture reads like the
    engine's own structure.
  - The same labels make Nsight Graphics, Radeon GPU Profiler and Intel GPA useful.
- **Frame debugger**, like Unity's, lighter than RenderDoc and always available. *(Implemented: the Frame Debugger
  panel, in the Profiling layout.)*
  - It lists the passes of the frame the editor just drew, in order, each with its GPU time, and the images the
    picked pass used and how: drawn into, sampled, depth tested, copied.
  - The picked image shows as it stood right after its pass, at any of its mips, in all its colour channels or one
    of them as grey, and brighter or darker by an exposure in stops. Depth starts six stops brighter, since it is
    held as small numbers.
  - Above them, how many objects the frame drew and how many were left out as outside the view or hidden behind
    others.
  - Unity's steps through draws; this steps through the render graph's passes, which is how a Voxear frame is
    described. The renderer notes the passes only while the panel shows (`Renderer::set_frame_debugger`). Looking
    at an image costs one copy and one small pass a frame: after its pass, the graph copies the mip being looked
    at, and a pass draws it into a view image with the channels and exposure asked for, which the panel shows as a
    UI image (`FrameGraph::insert_pass`, `frame_debugger.rs`). The view is a frame behind for passes after the UI.
  - Every image the graph owns can be copied from, and so can the window's own where the system allows it.
  - Still to come: buffers, the meshlets culled and the LOD chosen, and pipeline state.
- **Captures** *(implemented)*: `renderer.request_capture(Capture { area, before_ui })` asks for an area of a coming
  frame, as the view is before the UI draws over it or as the window shows it, and `take_capture()` hands it back as
  RGBA8 sRGB a frame or two later, once the GPU is done with it (`capture.rs`). Several come in the order asked for,
  so asking each frame makes a run of them, as for a GIF. It works in a window as well as offscreen, where the system
  lets the window's image be copied. VoxDocs captures the Scene view into its pages this way.
- **CPU and GPU profiler.** *(The editor's Profiler has the hierarchy and timeline views, cost per system and per
  behaviour callback, scripts' own samples, managed allocations, and GPU time per pass; see the editor above.)*
  - Timeline and hierarchy views. The recorder keeps each run of a sample as a span, with when it started and ended
    in the frame and how deep it ran (`profiling::Span`), beside the merged tree; the Timeline draws them as bars,
    coloured as the engine's work, scripts, rendering or waiting, names the widest, and describes the one under the
    pointer.
  - Cost per system, per job and per behaviour.
  - GPU time per render graph pass, from timestamp queries.
  - Managed allocations: each sample a script begins notes the bytes its thread had allocated, and ends with what it
    allocated since (`profiling::end_with_alloc`), so the Hierarchy has a GC Alloc column, as Unity's does, and the
    summary the frame's total.
  - The stats overlay (F1) graphs the last few seconds of frame times under a line at 60 frames a second, so a hitch
    shows as a spike.
  - Worker threads: a thread that names itself with `profiling::register_thread`, as the one a level loads on does,
    has its samples kept while the Profiler records. Each frame takes those that ended in it and those still running,
    cut to the frame, so a load lasting seconds shows in every frame it runs through. The Timeline draws each worker in
    a lane of its own under the main thread's, titled with its name, and pointing at a bar says which thread it ran on.
  - **Tracy** *(implemented: zones, threads and frames)*, for deep sessions: built with `cargo build --features tracy`,
    every sample is also a zone in [Tracy](https://github.com/wolfpld/tracy), on every thread and whether or not the
    Profiler records: the engine's systems, scripts' callbacks and their own `Profiler.Sample`s, and the workers',
    with each thread's name and a frame mark at each frame's end. It needs Tracy's viewer at version 0.14. The client
    collects only while the viewer is connected, and only from this machine, without announcing itself on the
    network. Locks, plots and frame images are still to come.
- **Memory profiler** *(first part implemented: the Memory panel, `memory_panel.rs`)*.
  - A snapshot (`voxear_engine::memory::snapshot`) lists what each part of the engine holds, by name: the process as
    the system counts it; Rust's heap, which `vox` and the player count with `CountingAllocator` as their global
    allocator, two atomic sums an allocation; the ECS's chunks by archetype; every GPU allocation by name, and the
    room its blocks have unused (the allocator's report); and the managed heap, in use and committed, from .NET.
  - The Memory panel, in the Profiling layout, keeps a line of the parts' totals up to date while it shows, and
    takes snapshots by hand, each labelled with the level open. A snapshot picked shows its amounts, the largest
    first; compared with another, only what changed, the largest change first, as before and after a level change
    to find what was not let go (`voxear_core::memory::diff`). The amounts are a table under a header whose column
    edges drag (`editor::columns`), each column starting as a share of the width, and a name too long for its
    column is cut off at its edge.
  - Still to come: Rust's heap by subsystem (Content, render, audio, physics, scripts), which needs allocations
    tagged where they are made; "who holds this asset", the reference chain behind Content's refcounts; and a
    bundle view of what is resident and why.
- **ECS inspector** *(first part implemented: the Entities panel, `entities_panel.rs`)*.
  - Live archetypes, with how many entities and chunks each has, the busiest first; the picked one's entities; and
    the picked entity's components with their values (the Transform, a Name, and C# components by their layout).
    Picking an entity that is an Obj selects it, so the Inspector shows and edits it.
  - The system schedule: each stage's C# systems (which ScriptInfo lists, with their stage and order) and then the
    engine's, with their milliseconds when the Profiler is recording.
  - A filter of component names, as "Transform, Spin", keeps the archetypes that have them all: what a query over
    them would walk.
  - It reads the world a few times a second while it shows, in the Profiling layout beside the Profiler. Its four
    lists sit side by side in panes whose lines drag to share the room out.
  - A plain entity, one that is not an Obj, is changed in the panel, in the running world: its Transform, its Name, and
    its C# components' fields, typed as numbers, or a vector's or colour's numbers between commas. An Obj is changed
    in the Inspector instead, where it can be undone and saved. Values are left alone while one is being typed in.
  - The picked archetype's chunks show as a strip of bars, each filled as far as its chunk is, with how many there
    are, how many entities each holds, and how full they are together.
- **Debug draw** *(implemented)*.
  - `Debug.Line`, `Arrow`, `Box`, `Sphere` and `Text` from C#, each with a duration and a `Color` (sRGB, with
    `Color.Hex("#ff8800")`), and the same from Rust in `voxear_core::debug_draw`, which keeps one list for the process
    so code needs no world or renderer to hand.
  - A shape lasts its duration in game time, counted down at the start of each game frame, so a paused game keeps
    showing what was drawn; with none, it shows for the frame it was drawn in. The editor counts down itself while not
    playing, and clears it all when play stops.
  - **`Draw`** *(implemented)*, the full drawing API, callable from anywhere: lines, rays, arrows, polylines,
    crosses, circles, arcs, wire boxes, spheres, capsules, cylinders and cones, frustums, beziers, grids and labels,
    each with an optional colour. Nestable `using` scopes set the colour, a matrix (`InLocalSpace(obj)`), the width
    in pixels, a duration, on top, and `InGame()`, whose lines are kept in release builds, where debug draw is
    otherwise off (`LineStyle::in_game`). The scopes are per thread. Shapes are made of lines in C#, each sent
    through the same native call as `Debug.Line`, whose flags carry the width in sixteenths of a pixel (bits 8 to
    23), so the native API did not change.
  - The renderer draws the lines after post-processing, so their colours are exactly as asked, as quads with soft
    edges, each as wide as it asks or a pixel and a half (`debug_pass.rs`, `debug.slang`). It reads the scene's depth
    to dim them to 30% where the scene is in front, unless drawn on top. Texts are labels at their points over the 3D view, as the camera sees them: over the
    editor's Scene and Game views, and over a player's window, under its stats and console (`debug_texts.rs` in
    voxear_engine, which both share).
  - Calls are `[Conditional("DEBUG")]`, so scripts built for a release leave them out, and a release build of the
    player turns debug draw off.
- **Debug views.**
  - Unlit, wireframe, and lit wireframe: the lit scene, then each mesh's edges drawn over it as faint lines, a
    little in front of its own faces so they are not lost in them (the Edges pass) *(implemented)*.
  - Albedo, normals, roughness and overdraw.
  - LOD and meshlet colouring, light complexity, GI voxels and shadow cascades.
  - Physics colliders, nav graphs, audio occlusion rays and AI paths.
- **Dev console** *(implemented)*.
  - In a game window with its tools (`vox run`, and development builds), the key left of 1 opens a console over the
    top of the game, as games have had since Quake: the log as it comes, warnings and errors in their colours, and a
    line to type into, with Up and Down for what was typed and Tab to complete a name (`dev_console.rs`).
  - A line runs a `[Command]` method by its name with its arguments, as `SpawnWave 3 Wolves` (text, toggles,
    numbers, enums by name, vectors as "x,y,z", and defaults for what is left out), printing what it returns; or
    reads or sets a `[ConsoleVariable]` static field or property, as `God` and `God true`. `help` lists them with
    their help, `clear` empties what shows, and `quit` closes the game. Names match ignoring case.
  - Scripts parse and run the line (`DevConsole` in Voxear.Core), and what it did is logged, so the console shows it
    as it shows everything else; the latest log lines are kept for it in `voxear_core::logging::recent`.
  - `vox console "God true"` runs a line in a game running with its tools, from `vox run` (`--project`) or a
    development build (`--player`, its Data folder), and prints what it logged, warnings and errors marked. The game
    listens on the loopback address only, on a port the system picks, and keeps the address and a secret in a
    `Console` file only its user can read, in the project's `.voxear` folder or the build's Data folder; it removes the
    file when it closes. A connection without the secret runs nothing (`remote_console.rs`).
- **Debuggers.**
  - Rider, Visual Studio or VS Code attach to the running engine to debug C#, since CoreCLR supports debugging a
    hosted runtime. The editor writes the launch configurations.
  - Rust debugs normally with gdb, lldb or Visual Studio.
- **Validation and GPU crashes.**
  - Vulkan validation, including synchronisation validation, toggles from settings.
  - On a lost device, `VK_EXT_device_fault` (and NVIDIA Aftermath when present) reports which pass crashed the GPU.
- **Crash reports.** A crash handler writes a minidump, the log, breadcrumbs and system info (GPU, driver, OS).
- **Remote attach.** Every panel can attach to a development build running on another machine, like Unity's "attach
  to player".

## Build pipeline

`vox build` *(implemented)* turns a project into a game players can run, in three stages
(`voxear_engine::player_build`):
1. **Scripts**: the game's C# in Release, or in Debug for a development build, with the profile's defines and
   the built-in ones (`scripts::built_in_defines`): `VOXEAR`, the engine's version (`VOXEAR_0_1`, and
   `VOXEAR_0_1_OR_NEWER` for it and each minor version before it in its major, with `VOXEAR_1_OR_NEWER` and the like
   from major 1), the platform (`VOXEAR_LINUX`, `VOXEAR_WINDOWS`), and `VOXEAR_DEVELOPMENT` for a development build.
   The editor's own build, which `vox run` shares, has `VOXEAR_EDITOR` in place of that. A build's scripts are
   built into `.voxear/Scripts/Build/<configuration>` with their own intermediate folder, so a build never replaces
   the editor's copy with one built without `VOXEAR_EDITOR`.
2. **Content**: the bundles the profile's levels need (see the content build), with the `[ContentRoot]`s of the
   scripts just built. Cooked models come from the content cache, so an unchanged model is not cooked again.
3. **Player**: the player program, named after the game, with `Voxear.Core` and the scripts beside it.

It ends by writing `build-report.json`. Later stages: shader permutations once there are quality tiers, and
NativeAOT players.

### Build profiles

What to build is a **build profile**, like Unity 6's Build Profiles: a `.buildprofile` TOML file under
`Settings/BuildProfiles/`, named by its file. A project can have as many as it likes, such as `Windows Release`,
`Linux Demo` and `Steam Playtest` (`voxear_engine::build_profile`).

**Easy.** A new project comes with a Release profile and a Development profile for each platform, and `vox build`
gives a project without any the same four. Pick one and press Build, or run `vox build --profile "Linux Release"`.
Without `--profile`, `vox build` builds this platform's Release profile.

**Advanced.** A profile only lists what differs from its base, the same way post-processing presets layer. Tables
merge key by key, so a profile can change one player setting and keep the rest of its base's:

```toml
format = 1
base = "Linux Release"       # optional: change another profile
platform = "linux-x64"       # or "windows-x64"; the machine building when not set

[levels]
start = "Levels/Menu.level"         # default: voxear.toml's start_level
include = ["Levels/Demo/*.level"]   # default: every level

[development]
enabled = true               # scripts in Debug with their symbols, and vox run's tools
validation_layers = false    # Vulkan's validation layers, when installed

[scripting]
defines = ["DEMO"]           # for C#'s #if

[player]
product_name = "Orbit Demo"  # the program's name and window title; the project's by default
company_name = "Orbit Games"
window = "windowed"          # or "fullscreen", borderless
width = 1600
height = 900

[version]
version = "1.2.0"

[content]
compression = "zstd"         # or "lz4", quick to load, or "none"; the project's own when not set

[output]
folder = "Builds/Demo"       # default: Builds/<profile>
```

A level left out of `include` stops being a root of the content build, so what only it uses does not ship. The
start level always ships. A base that names itself through others, an unknown platform or window mode, and keys a
profile does not have are reported with the profile's name.

The full set of options a profile is meant to carry, with what is still to come:

| Group | Options |
| --- | --- |
| Platform | Target OS and architecture. Linux and Windows x64 now; arm64 later. |
| Levels | Start level and which levels ship. Later: by default, what the start level can reach. |
| Development | Debug scripts and the tools, and Vulkan validation. Later: assertions, the debug panels and console, profiler autoconnect, remote attach, script debugging and deep profiling. |
| Scripting | Extra C# defines. Later: the backend, CoreCLR (fast builds, allows script mods) or NativeAOT (fast start-up, smaller, harder to reverse, no script mods), IL trimming, and shader defines. |
| Content | Bundle compression: LZ4 for load speed, Zstd for size, or none. Later: texture quality caps, which quality tiers' shader variants ship, and asset stripping rules. |
| Player | Product and company name, window mode and size. Later: icon, splash, vsync, run in background, and the crash reporter. |
| Version | The version string, and the git commit stamped into the player and the report. Later: an auto-incremented build number. |
| Graphics | Later: defaults per quality tier, including the post-processing preset. |
| Mods | Later: whether mods load at all, which trust tiers are allowed, and whether to export the mod SDK alongside. |
| Store | Later: Steam app and depot IDs, the branch to upload to, and whether to upload when the build succeeds. |
| Output | The folder. Later: a file naming pattern, clean or incremental, and Build and Run. |
| Hooks | Later: steps before and after each stage, a C# `IBuildStep` or a shell command, with the build's context as JSON. |

Switching the active profile is instant. Content is cached per platform, and Linux and Windows share desktop
texture formats, so there is no Unity-style reimport when changing platform.

**Builds for other platforms.** For now a build runs on the platform it is for. Player runtimes will ship prebuilt
in each engine install, per platform, as installable modules that the launcher manages. Building a Windows player on
Linux will then compile no Rust: it builds content and scripts, then packages the Windows runtime. Script builds for
CoreCLR are platform-neutral IL. NativeAOT can only compile on the target OS, so the profile says so up front and
offers a remote build agent once those exist.

### The player *(implemented)*

A build's folder holds the program and its `Data` folder:

```
Builds/Linux Release/
  Hello                  the player, voxear-player under the game's name
  Data/player.toml       what it runs: the start level, the scripts, the window, and what built it
  Data/Bundles/          the built content
  Data/Managed/          Voxear.Core and the game's scripts (with their symbols in a development build)
  build-report.json
```

The player (`voxear_engine::player`) opens the bundles, starts .NET with the game's scripts, and runs the start
level in a window of the profile's size, or full screen. A development build has the tools `vox run` has: the stats
overlay on F1, the debug views and the fly camera; a release build has none of them. `--headless --frames <n>` runs
it without a window, to check a build. It keeps its log as Unity's `Player.log` is kept, without the terminal's
colours, in the user's data folder under the company's name and the game's (`$XDG_DATA_HOME` or `~/.local/share`
on Linux, `%LOCALAPPDATA%` on Windows), with the run before's as `player-prev.log`, so the log outlives the game
and the editor's Console can read it. The player uses the .NET installed on the machine for now; shipping the
runtime with it, or NativeAOT, comes with M12.

The profile's window is how the game starts. Scripts read the window and ask of it through `Screen` (`Width`,
`Height`, `Fullscreen`, `SetResolution`) and `Cursor` (`Visible`, and `Lock`: `None`, `Locked` or `Confined`), in
`managed/Voxear.Core/Display`, native API 25. The game keeps them in `Game::display`: whatever shows the game sets
the window's size before each frame, and after it makes what the scripts asked for, a new size or full screen, and
the pointer shown, hidden and held (`windowed::App::apply_display`; the fly camera's own hold comes first, and the
game's comes back after it). In the editor, they read the Game view's size, and the window and the pointer are left
as they are, so the editor can still be worked in.

### Builds run in their own process

The editor never builds inside itself. Pressing Build starts `vox build --profile <name> --progress json`, the same
command CI runs, and the Build window follows it. This means:
- The editor stays responsive, and you can keep working while it builds.
- A build that crashes cannot take the editor with it.
- A build behaves the same from the editor, the terminal and CI.
- Later, several profiles can build at once, one process each, sharing the content cache under a lock.

The process reports progress as one JSON object per line on stdout, its log lines among them, and nothing else:

```json
{"event":"stage","stage":"content","index":2,"of":3}
{"event":"progress","stage":"content","done":120,"total":800,"item":"Textures/Rock.png"}
{"event":"log","level":"warn","message":"[ContentRoot(\"Icons/*\")] on Hud matches no asset"}
{"event":"finished","result":"succeeded","seconds":42.7,"output":"…/Builds/Linux Release","program":"…/Builds/Linux Release/Hello","size":31457280,"report":"…/build-report.json"}
```

A failed or cancelled build finishes with `"result":"failed"` or `"cancelled"` and a `message`, and exits with 1. A
line `cancel` on stdin stops the build at its next step, between stages or assets, which leaves every cache
consistent.

**The Build window** *(implemented)* opens from the top bar's Build button, by the Scene view. It lists the
profiles on the left and the picked one's settings on the right, like Unity 6, and offers the default profiles to a
project without any. A running build shows its stages with their times, a progress bar for the content stage, and
the log, warnings and errors in their colours. Cancel asks the process to stop, and stops it if it has not within
ten seconds. A finished build says how long it took and how big it is, in the window, the status bar and the
Console, and offers Run and Show in Files. The picked profile's settings are edited in place: words, lists and
numbers once Enter is pressed or the field is left, toggles and choices at once, each written to the profile's own
file (`build_profile::set_setting`, keeping its comments). A value its base already has is taken out again, so a
profile lists only what differs. The start level and the levels that ship are level fields, as the Inspector's
object fields are: a click offers the project's levels, a level dragged from the Project panel lands, and they are
written by GUID, which the build matches as it matches a path or pattern, so a renamed level stays named. The levels
and the defines are lists of fields, each with a button to take it out, and one to add one. New makes a profile
based on the picked one (`build_profile::add_profile`), its name typed in the title, which renames it and every
profile based on it (`rename_profile`); Remove moves it into `.voxear/Trash`, unless another is based on it
(`remove_profile`). Buttons over the log show or hide its info, warnings and errors, with their counts,
and a build that finishes while the editor is not the window in front says so on the desktop (`notify-send` on
Linux). Picking a log line that names a file in the project, as `cooking Models/Rock.gltf` does, shows it in the
Project panel.

**The build report** comes with every build: the project, profile, platform, engine version and git commit, the
levels shipped, each bundle's size and roots, the build's size, and each stage's time. Later it gains:
- Size by asset and type.
- Why each asset was included: the chain of references from a level.
- Warnings and shader variant counts.
- A diff against the previous build of the same profile, so a size jump points straight at its cause.

## Versions, installs and the launcher

The **Voxear Launcher** is the equivalent of Unity Hub. It lives in its own repository and works with the engine
through a small contract.

**Engine versions.**
- Each engine release is a self-contained, versioned install: editor, `vox`, `Voxear.Core`, templates and docs.
- Releases are published with a **release manifest**: version, channel (stable, beta or nightly), downloads per
  platform, sizes, SHA-256 hashes and a signature.
- Several versions install side by side. `vox` finds its own files through its install folder (`VOXEAR_HOME`).

**Projects pin a version.**
- `voxear.toml` records the engine version a project uses.
- The launcher opens a project with that version, and offers to install it if it is missing.
- Upgrades make a backup first and summarise what changed.

**New project flow.**
- Templates ship with each engine version and are applied through `vox new`, so the launcher, the CLI and the editor
  create identical projects.
- The flow asks for:
  - name and location
  - template: Empty, 3D, 2D, First-person, Third-person or Top-down
  - engine version
  - a **default setup**: lighting and sky, camera, input actions, a sample scene and quality presets
- Optional extras: C# IDE setup (Rider, Visual Studio, VS Code), git with a `.gitignore`, modding switched on, and a
  Steam App ID.

**Launcher self-update.** The launcher updates itself from its own signed manifest, separately from engine versions.

**What exists now.**
- **Installs find themselves.** `engine_root()` is `VOXEAR_HOME` when it is set, as the launcher sets it; otherwise
  the folder above the running program's `bin/` when that holds an `engine.toml`; otherwise the source checkout.
  An install keeps `Voxear.Core` and `Voxear.ScriptInfo` in `managed/` (`core_assembly()`); a checkout keeps
  them in `target/managed/<configuration>/`.
- **Scripts build against the engine running them.** `managed/Voxear.Scripts.props` holds what a game's scripts
  project needs: the framework and language settings, and Voxear.Core (the built library in an install, the
  project in a checkout). The editor and `vox` pass the engine's folder to every scripts build as `VoxearHome`,
  and a scripts project imports the props from there, so a project builds wherever it lives. Hello falls back to
  the checkout it sits in.
- **Projects can be pinned.** `engine` in `[project]` is the version the launcher pins; a different version warns
  when the project opens.
- **`vox new`** makes projects for the launcher's New Project form (`voxear_engine::new_project`): `--describe
  --json` says what this version offers, and `vox new <folder> --name … --template … [--setup …] [--ide …]
  [--git] --json` makes the project, reporting each step as a line of JSON. The templates are Empty and 3D, with
  the setup items lighting and sky, camera, and a sample scene (a ground and a cube turned by a script); the
  first-person, third-person, top-down and 2D templates wait for an input API and 2D rendering. A project gets
  `voxear.toml` pinned to this version, a level and its scene, a scripts project and a solution, and its assets'
  `.meta` files; the IDE choice adds VS Code's settings, and `--git` a repository that leaves `.voxear/` out.
  Modding and a Steam App ID are reported as not there yet. Every scripts build writes the project's
  `.voxear/Engine.props`, naming the engine it built with, which a scripts project imports when nothing else
  names one, so an IDE builds it too.
- **`vox upgrade <folder> [--from <version>] [--json]`** moves a project onto this version, as the launcher asks
  once it has backed the project up (`voxear_engine::upgrade`; `[supports] upgrade = 1`). It stops before changing
  anything if a level, scene or prefab was saved by a newer Voxear; then it pins `voxear.toml` to this version,
  keeping the rest of the file, writes `.voxear/Engine.props`, adds the default build profiles to a project without
  any, and gives new assets their `.meta` files. Each change is said in plain words (`{"event":"change"}` lines
  with `--json`, ending in `done`, or `error` and a failing exit), and logs go to stderr. Later versions add their
  conversions there.
- **`voxear-editor`** opens a project as the launcher asks (`--project <folder>`), handing over to `vox editor`
  beside it, so an install carries the engine once. `vox --version` gives the release's version.
- **Packaging** (`voxear-dist`, `cargo run -p voxear_dist -- --channel nightly`): builds `vox`, `voxear-editor`
  and `voxear-player` in release (naming the version through `VOXEAR_VERSION`), and Voxear.Core and ScriptInfo in
  release; lays out the install in `target/dist/voxear-<version>-<platform>/` with `engine.toml`, `bin/`,
  `managed/` (with the scripts props), `docs/`, `manual/` and the licence; and packs it as a `.tar.zst` with every
  entry at the root, in name order and without owners or times, all but the manual, which goes in an archive of its
  own, `voxear-<version>-manual.tar.zst`, the same on every platform, for the launcher to offer as an optional part
  of an install. A nightly is named for the day (`0.1.0-nightly.20261031`);
  a beta names its version. The folder is an install too, which `voxup locate` can point the launcher at. Run on
  Windows it packages for Windows (`voxear-<version>-windows-x86_64.tar.zst`), with the C runtime linked into the
  programs so an install needs nothing else of Microsoft's; it builds wherever `CARGO_TARGET_DIR` points.
- **Publishing** is by hand, with the launcher repository's `voxear-release`: `engine add` signs the archive with
  the engine key and lists it in the site's `engine/releases.toml`, then the archive and its `.minisig` go up as
  assets of the `engine-v<version>` release in VoxearReleases, and the site goes to its `gh-pages` branch.
- **The release workflow** (`.github/workflows/release.yml`, run by hand from the Actions tab) packages both
  platforms with one version, on Ubuntu 22.04 for an older glibc and on Windows, and keeps the archives as the run's
  artifacts. With Publish ticked it also does what is otherwise done by hand: `voxear-release engine add` signs both
  and lists them (pruning old nightlies), the archives and their signatures go up as the `engine-v<version>`
  release of VoxearReleases, and the manifest is pushed to its `gh-pages` branch, archives first. Publishing needs
  three secrets on the repository: the engine key's file, its password, and a token that can read the launcher
  repository and write to VoxearReleases. Nothing runs on a schedule, since each run builds the engine twice on
  paid runners.
- **Not yet tried:** the workflow itself, and so the Windows build, which wait for a first run on GitHub.

## Platforms and GPU support

**Targets.** Linux (Wayland and X11) and Windows 10/11 on x86-64. These are the only platforms for now.

**GPU support.** Any GPU with Vulkan 1.3 and the features listed above: NVIDIA (proprietary and NVK), AMD (RADV and
AMDVLK), and Intel (ANV and Windows drivers). `vox doctor` explains exactly what a GPU lacks.

**Optional tiers.** Mesh shaders, hardware ray tracing and DLSS/XeSS. Every feature has a fallback.

Other platforms (macOS, mobile, consoles, XR) are out of scope for now. The RHI still keeps platform code
contained, so adding one later does not mean reworking the engine.
