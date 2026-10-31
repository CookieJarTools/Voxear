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
| Mesh import | glTF 2.0 first, then FBX via ufbx | meshoptimizer handles meshlets, LOD and vertex cache optimisation. |

## Conventions

- **Coordinates match Unity:** left-handed, **Y up**, **Z forward**, X right, units in metres. Positive rotation
  follows the left-hand rule, so a positive yaw turns to the right. Scripts and scenes ported from Unity keep their
  numbers.
- Rotations are quaternions. Euler angles in the inspector are in degrees and applied in Z, X, Y order, as in Unity.
- Importers convert right-handed formats such as glTF and FBX so that assets face +Z, and nothing is flipped at
  runtime.
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

// Advanced: a C# system over chunk spans, no per-object calls (planned for M3).
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

## Scenes, Levels and Scene Persistence *(implemented)*

- A **Scene** is a `.scene` TOML file (diff- and merge-friendly) holding Objs and their components. Each Obj has
  an `id` that is unique in the file, and `parent` refers to it. Rotations are Euler degrees, applied the way Unity
  applies them. Components are subtables:
  - `[obj.camera]`, `[obj.light]` and `[obj.renderer]`, the last with a mesh reference and a material reference or
    inline material
  - `behaviours = ["Game.Spinner"]`
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
      least this long, so a loading screen is seen), `UnloadRest`, `SetActive` and `TidyUp` (free what nothing uses
      once it is over); `LoadAsync(level, activate: false)` is the same as `Held`

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
    play-mode run in the editor (scripts are not reloaded between runs yet), so listeners should unsubscribe in
    OnDestroy.

  - **How it works.** A worker thread with its own reader works from the project folder or from bundles. It reads and
    parses every level, scene, prefab and material file the level needs, and prepares its models: parsing glTF,
    decoding images and building LODs. The main thread uploads the prepared models within a 4 ms budget per frame,
    then changes level from the texts already read, with no IO left to do. Anything already loaded is skipped. In
    Rust it is `Game::load_level_async`, `Game::load_status` and `Loads::set_allow_activation`; the code is in
    `crates/voxear_engine/src/loading.rs`.
  - Assets that behaviour fields name (`Prefab` and `Ref<T>` fields) are not preloaded, since they load on their
    own.
- **Scene Persistence:** Objs marked `persistent` (the `ScenePersistence` component) survive every level change,
  along with their children, like `DontDestroyOnLoad`. `obj.Persist()` will be the easy form in code.
  - **No duplicates on reload, unlike Unity.** Reloading the scene a persistent Obj came from does not create a
    second copy. The engine remembers which scene and id the survivor came from and skips it, so there is no "destroy
    myself if a copy exists" boilerplate.
- From C#, `Level.Load("Levels/Night.level")` changes level after the current frame. Unloaded Objs stop rendering
  at once, and their behaviours get OnDisable and OnDestroy at the end of the next frame.
- Loading a scene is one lifecycle batch, so ordering guarantee 2 holds across the whole scene.
- **Objs created at runtime**, such as instantiated prefabs, belong to the level's active scene and unload with
  it, unless they are persistent. Objs marked `EditorOnly`, such as the editor's Scene camera, belong to no scene
  and level changes leave them alone.
- **The Levels panel** in the editor is LayerCake's window: every level (with its scene count and the level it
  stands on) and every scene in no level, to find by name. Stopped, clicking a level opens it to edit (a loose scene
  opens on its own) and its play button opens it and plays. Playing, clicking a level changes to it through the
  scripts' `Level.LoadAsync`, so every event fires and the game behaves as it would in a build; the bar at the
  bottom follows any level change, whoever started it.

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
- arrays and `List<T>` of any of these
- **`Prefab`:** a direct reference, loaded with the Obj, before Awake.
- **`Ref<T>`:** a lazy reference, like an Addressables AssetReference. It holds the path or GUID, and
  `rareFirefly.Load()` loads it when the game first needs it. `T` is any `IAsset<T>`, which `Prefab` is today.
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

## Content (assets, Bundles, loading)

This is the only way to load assets. It replaces Addressables and fixes the build pain by making bundling automatic.

**Authoring.**
- Every asset has a stable GUID in a sidecar `.meta` file, for example `Ship.glb.meta`. Moving a file together with
  its `.meta` keeps its GUID, and a copied file and `.meta` get a fresh one *(implemented in `voxear_assets`)*.
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
   prefab, and every asset a behaviour field names.
   - A field's asset is a root of its own because `Ref<T>` loads it separately. A `Prefab` field is treated the same
     way, so its bundle opens at the Obj's Awake.
   - Dependencies come from the files: a level's scenes, the prefabs, meshes and materials that scenes and prefabs
     use (overrides included), and the textures a model names. Files a model reads that are not assets, such as its
     `.bin` buffers, are packed with it.
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
   - A bundle is a header followed by each file, LZ4-compressed when that saves space. Cooked models are stored as
     they are, since they compress their own GPU data (below). A bundle is named after a hash of
     its bytes, so a rebuild that changes nothing writes nothing, and bundles no longer listed are deleted.
   - The **catalog** (`catalog.toml`, readable on purpose) maps each asset's GUID and path to its bundle, offsets
     and sizes. It also lists each bundle's usage set and the bundles it depends on.
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
     - *Planned:* the other BC7 modes for sharper colour, BC6H for HDR textures, and cooking for other asset
       kinds.
   - Other assets (levels, scenes, prefabs, materials) ship as their source text, LZ4-compressed.

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
- **Selected on the GPU by screen-space error**, not by distance. The same mesh picks the right LOD at any FOV or
  resolution.
- **Transitions are dithered cross-fades**, smoothed by TAA, so LOD changes do not pop.
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

**Shadows.** Cascaded shadow maps first, then virtual shadow maps. Soft shadows use PCSS, and contact shadows cover
small detail.

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
| `anti_aliasing` | `mode` (`taa`, `fxaa`, `off`), `sharpness` 0.25, `taa_history` 0.9, `taa_jitter` 1.0 |
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

## UI (Voxear UI) *(implemented: tree, layout incl. grid, text, drawing, markup, style sheets, transitions, pointer and keyboard, scroll/list/tree views, input controls, split views, tabs, dropdowns, docking, default theme; C# planned)*

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
  `:disabled` and `:checked`, and descendant and `>` combinators. `:root` is the element a sheet is attached to,
  so each document keeps its theme tokens at its own top.
- **The cascade:** the style given in code first, then matching rules from least to most specific (a nearer
  sheet, then a later rule, wins a tie), then inline declarations. Text styles and custom properties
  (`--name`, read with `var()`) are inherited, as in CSS.
- **Properties:** CSS's names for flexbox layout, sizes, margins, padding, gaps and positions, plus
  `background-color`, `border`, `border-radius`, `opacity`, `pointer-events`, `color`, `font-size`,
  `font-weight` and `font-family`. Lengths are `px` or `%`. Colours are hex, `rgb()`, `rgba()`, `oklch()` or a
  few names.
- **Mistakes** are reported once each, with the file, line and what was wrong, such as
  `Stats.vss:12: unknown property 'colour'` or `Test.vui:2:3: unknown element <Lable>`. A broken rule is
  skipped, as browsers do, and the rest of the sheet still applies.
- **The pointer** sets `:hover` on the element under it and everything it is in, and `:active` while pressed.
  A press and release on the same element is a click, which code reads with `ui.clicked(node)`, as the overlay
  does to fold itself away. Disabled elements cannot be clicked, and `pointer-events: none` lets the pointer
  through. A document's container lets it through too, so UI over a game takes only the clicks it is under.
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
  scrolls. Rows match `:selected`; a click selects, Ctrl adds or removes, and Shift extends.

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

**Input controls** *(implemented)*. Each is made of ordinary elements with classes, so a sheet can restyle
every part:
- **TextField**: one line, or several with `multiline`. Editing is parley's own: the caret and word movement,
  selection by drag, double click (a word) and triple click (a line), Ctrl+A/C/X/V, Ctrl+Backspace and Delete
  by word, a placeholder while empty, and input-method composition with `ime_area` for the candidate list. A
  long line scrolls to keep the caret in view, and the text is clipped to the field.
- **Toggle** (a switch: a 28 by 16 pill with a sliding knob, or a ticked box with the `checkbox` class),
  **Slider** (dragged, or moved by the arrow keys, with an optional step, and its value shown in mono to as many
  decimals as the step has) and **Foldout** (a header with a chevron that opens and closes its content; markup
  children go inside it).
- **Segmented**: a row of segments, each text, an icon or both, exactly one on, for modes such as the transform
  tool or the pivot (`<Segmented value="1"><Segment icon="move" />...</Segmented>`).
- **Focus and the keyboard:** clicking a control focuses it, Tab and Shift+Tab walk the controls in tree order
  (skipping hidden and disabled ones), and the focused one matches `:focus`. Enter and Space press a focused
  button or toggle, and Escape lets go. While a text field has focus, keys belong to it (`wants_keyboard`),
  so typing a W never moves the camera.
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
  dropdown's text style. A choice, Escape or a press anywhere else closes it, and that press goes no further.
  The arrow keys change the choice while it is focused.

**Docking** *(implemented)*, the editor's layout: a `DockSpace` is a tree of split panes whose leaves are
tabbed areas of panels.
- Dragging a panel's tab onto another area's middle adds it there as a tab; onto an edge (the outer quarter),
  it splits that area. A ghost of the tab follows the pointer and a preview shows where it will land. Areas
  left empty fold away, and a press that never moves stays a click on the tab.
- A panel keeps its own elements wherever it goes, so a half-typed field or a scrolled list survives a move.
- The layout reads and writes as text, for workspace presets and saving between sessions:
  `row(240, [hierarchy, project], column(420, [scene, game*], [console]))`, where `row` and `column` give the
  first part's size in pixels (or the second's, negative, or the first's share, as `57%`) and `*` marks the
  showing tab. Divider moves and tab choices are part of it.

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

**Icons and shapes** *(implemented)*:
- **Icons** are Voxear's own line icons (`voxear_ui::ICONS`: chevrons, play controls, tools, object and asset
  kinds, the Voxear mark), drawn on a 24-unit grid as SVG path data. Each is rasterised by swash's zeno into the
  glyph atlas at its exact pixel size, its lines 1.5 physical pixels wide at any size, so it is as crisp as text,
  and drawn in its element's text colour. `<Icon icon="move" />`, `ui.icon(parent, "move")` or
  `ui.icon_button(parent, "plus")`; a Button in markup may hold an icon and a label.
- **`box-shadow`** (`0 16px 40px rgba(0,0,0,.4)`, with an optional spread) draws a Gaussian-blurred copy of the
  box under it, cut out of the box as CSS's is. A spread with no blur makes a crisp ring, and a 1px offset a
  hairline under a header.
- **`border-style: dashed`** (or `dashed` in `border`) dashes a border, for drop targets such as Add Component.
- **`line-height`** sets the height of each line of text, so 12px text sits on a 16px line as in the design.
- **`z-index`** above 0 draws an element and its children after everything else, higher ones last, as for the
  split handles. The pointer still finds elements in tree order.
- **Shapes** (`ui.set_shapes`): lines with round ends and circles drawn on any element, for gizmos and graphs.
- **Rich text** (`RichText`, `ui.rich_label`): a label whose words are bold, italic, code, struck out or
  highlighted, with links that `ui.clicked_link` reports. Each span shapes as its own run, so its colour, ground
  and lines are drawn with it; `link-color`, `code-background` and `mark-color` come from the text style, and
  italics and bold are slanted and thickened when a font has no such face. `white-space: nowrap` keeps a line
  whole, as list rows do.
- **Icons in tabs and foldouts:** a `DockPanel`'s `icon` (or `add_tab_with_icon`) shows before its title, and
  `set_foldout_icon` before a foldout's; `foldout_header` takes buttons or a switch after the title, which work
  without opening or closing it.

**The default theme** (`crates/voxear_ui/theme/Default.vss`) is on every `Ui`'s root: the editor's design
(see the editor's design below), with its tokens as `--vx-*` variables, for buttons, fields, switches and
checkboxes, sliders, segmented controls, foldouts, list rows, split dividers, tabs, dropdowns and docking, with
120 ms ease-out transitions on what the pointer touches. Text is 12px Inter on a 16px line. A sheet of your own
overrides any of it, and redefining the `--vx-*` variables is the quickest way to retheme. `Ui::bare()` starts
without it.

**Still to come.**
- Keyframe animations, beyond transitions.
- Floating windows that panels can be torn off into.
- The C# API, so game UI and editor extensions are written in C#.
- Data binding to C#, using source generators instead of reflection.
- A live-reloading UI builder in the editor.

**Crisp text** uses two paths, the same split Chrome uses:
- **Screen-space and editor text** is rasterised at the exact physical pixel size with hinting. It goes into an
  atlas keyed by font, size and glyph, with the baseline snapped to whole pixels, and a coverage curve tuned for
  light-on-dark text is applied. This is what makes small editor text sharp. *(Implemented, with glyphs on whole
  pixels; 4 sub-pixel x positions come with light hinting.)*
- **World-space and large or transformed text** uses MSDF, with an option for Slug-style curve rendering (now patent
  free). It stays sharp at any distance and angle. The renderer switches paths on scale and perspective.

**Rendering.** Shapes are instanced SDF quads, so rounded rectangles, borders, shadows and gradients stay crisp at
any DPI. World-space panels render to a mipmapped texture sized to their on-screen pixel density, then draw as
surfaces in 3D.

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
    state carries over, as in Unity with domain reload turned off. A change to the scripts needs the editor
    restarted until C# hot reload comes.
- **The top bar** has the Voxear mark, the workspaces, the command bar (Ctrl+K), play, pause and step in a pill,
  the user's avatar and the Build button. A **rail** of icons runs down the left, with source control at the
  bottom.
- **Docked panels:** the Hierarchy (Objs by parent, with type icons, child counts and a filter), the Scene view and
  the Game view (through the level's first camera, and the one drawn while playing when both show), the Project and
  the Console under them, and the Inspector on the right at 300px. Each panel has a
  32px header of tabs (the Scene's and Game's with icons) and its own controls on the right: the Scene's are its
  shading menu, Gizmos and full screen. They can all be dragged around, as docking allows.
- **The Project** is laid out as Unity's two-column Project window: the folders as a tree on the left, and the open
  folder's folders, assets and C# scripts on the right, with a breadcrumb and an item count. With room for three
  or more, they show as tiles; narrower, as rows with each file's size. Selecting a folder in the tree opens it,
  as does double-clicking one on the right. The Project sits under the Scene by default, because its two columns
  need the width; dragged under the Hierarchy, it turns to rows.
- **The Scene view** renders into its panel through the editor's own camera, flown with the right mouse button
  as in Unity. The renderer draws the 3D view into a rectangle of the window (`Renderer::set_view_rect`), clears
  around it to the window's background (`Renderer::set_clear_color`), and looks through a chosen camera
  (`ViewOptions::camera`). A frame drawn over its edge rounds its corners to the panel's. Over it float a tool
  strip (select, move, rotate and scale, Pivot, Global and grid snap), a stats chip, and an axis gizmo that turns
  with the camera. The shading menu, Gizmos and full screen are not wired up yet.
- **Scene view tools** (`scene_tools`), as Unity's. A click selects the nearest Obj under the pointer: a ray
  against each mesh's bounds in the Obj's own space (`Renderer::mesh_bounds`; cooked meshes keep only a sphere,
  so they use the box around it), or a light or camera close to where it shows; a click on nothing clears the
  selection. Around the selected Obj, move has an arrow per axis and a middle that moves in the view's plane,
  rotate a ring per axis, and scale a block per axis and a middle that scales evenly. Handles are Voxear UI shapes
  projected through the same matrices the renderer uses (`ViewCamera`), stay the same size on screen, and light
  up under the pointer, which takes a press on one before the UI sees it. The strip picks the tool (or Q, W, E and
  R), Pivot or Center, Global or Local axes, and the snap distance (0.1, 0.25, 0.5 or 1 m); Ctrl while dragging
  snaps to it, to 15 degrees, or to tenths of the scale. A drag is one step to undo.
- **The Inspector** has a header with the Obj's icon, its name and an active switch, then rows for Tag, Layer and
  Static (shown, not yet stored: Objs have no tags or layers yet). The Transform and each component are sections
  divided by lines, each headed by its icon and title with an enabled switch (or, for the Transform, a menu) on
  the right, and with an 80px label column; position, rotation (in degrees) and scale are mono fields with coloured axis
  letters, applied as they are typed, and changes made elsewhere show in fields not being edited. A dashed Add
  Component button ends it. While playing it shows the running Obj, its behaviours included.
- **Saving** *(Ctrl+S)* writes the open level's scenes back to their files (`Levels::save`). Only scenes that changed
  are written, so untouched ones keep their comments; each keeps its own sky, prefab instances keep their paths and
  write only what differs from the prefab, and Objs made since the level opened go into its active scene. Hello's
  levels save back unchanged. Edits while playing are not saved, as in Unity. Opening another level with unsaved
  changes asks for a second click first.
- **Undo and redo** *(Ctrl+Z, Ctrl+Shift+Z or Ctrl+Y)* take back and make again edits to an Obj's name and
  Transform, from the Inspector or the Scene view's handles; typing into one field is one step, and so is a drag.
- **The status bar** shows a light, what the editor is doing (ready, building scripts, playing or paused, or that
  there are unsaved changes) and how long the project took to open, then the renderer, frame rate, memory and git
  branch.
- `vox editor --screenshot editor.png` renders it offscreen instead, with an Obj selected.

Making, deleting and reparenting Objs (with undo), adding components, the command palette and the Odin-style
inspector attributes come next.

**Look.** Unity's modern dark concept crossed with Linear:
- Surfaces are separated by elevation rather than heavy borders.
- Themes are generated in OKLCH from three inputs (base, accent, contrast), as Linear does, so light, dark and
  custom themes all stay consistent.
- Inter and Inter Display fonts.
- Subtle 1 px separators.
- Animations take 100–180 ms with ease-out curves and never block input.

**Layout.**
- Dockable tabbed panels: Hierarchy (with instance badges), Inspector, Project, Scene, Game, Console, Profiler and
  Lighting.
- Workspace presets along the top (World Building, Animation, Lighting, Profiling), as in the concept.
- Floating tool overlays in the viewport, like the Paint Texture panel in the concept.

**Console.** Modelled on CookieJarTools' [Sifter](https://github.com/CookieJarTools/Sifter), a Unity console
whose source is in `Dev/CookieJar` (design notes in `Docs/Sifter-Design.md`):
- Every part of a stack trace is coloured for what it is, and any frame can be clicked.
- Categories come from the messages themselves, with nothing to declare.
- The list survives a script recompile, which here means C# hot reload.

Read Sifter's design before building it.

### The Cookbook

A project's own documentation, in the editor: the CookieJarTools Cookbook for Unity, rebuilt as part of the
engine in its own crate (`crates/voxear_cookbook`). Its source and design are in `Dev/CookieJar`
(`Assets/CookieJarTools/Cookbook`, and the guide in `Docs/Cookbook Guide`).

**What exists now** *(first slice)*: a **Cookbook** tab beside the Scene and Game.
- **Pages are plain Markdown files** in the project's `Docs` folder, so they can be written in any editor and
  shared with git. A folder's own words are in its `_folder.md`. A page's header holds its id, title, tags, when
  it was updated and where it sorts; lines the Cookbook does not know are kept.
- **The Markdown** is CommonMark's everyday parts plus tables, checklists, bare links, `~~strikethrough~~`,
  `==highlights==`, GitHub-style callouts (`> [!NOTE]`, `[!TIP]`, `[!WARNING]` and the rest), heading anchors,
  and `[[Page]]` and `[[Page|words]]` links. It is read by hand, not by a library, into blocks of rich text.
- **Links** go to pages (by id, path or a title only one page has), to headings (`#checklists`, or
  `Page.md#hits` on another page), to paths relative to the page, to files in the project (shown in the Project
  panel), and to the web. A link to a page not written yet is drawn faded and underlined; clicking it makes the
  page beside the one it is in and opens it to write.
- **The panel:** the pages down the side as a tree, a box that finds pages by every word typed, back and
  forward, the page with its title, tags and date, and for a folder what is in it. Ticking a checklist box saves
  the page. Code blocks have a Copy button. **Edit** shows the page's Markdown to change and **Save** writes it,
  with today's date.
- `samples/Hello/Docs` shows it: how the sample works, and the guide to writing pages.

**Still to come**, as the Unity Cookbook has them:
- `[Documentation("id")]` on C# types and fields, with a button beside them in the Inspector, and excerpts of a
  page shown in it.
- The **Code Reference**: a page for every public type, from its `///` comments, with notes kept in
  `Docs/.code-notes`, and `[[code:Type]]` links.
- **Step-by-step guides**: `[CookbookAction]` and `[CookbookCheck]` methods, and built-in steps (make an asset,
  open a panel, run a menu item), as links a page runs and ticks, with each person's progress kept.
- **Docs Health**: out-of-date pages, broken links, and code with no page yet; and git's last change on a page.
- **Pictures**, once Voxear UI draws images: dropped in, captured from the Scene view, or recorded as GIFs of the
  Scene or Game; and **live assets** shown in a page.
- Tags to filter by, "On this page", a home page with recent pages, quick open from the command bar (Ctrl+K), and
  an editor with the page beside its Markdown.

**Speed.**
- A **command palette** on Ctrl+K reaches every action, asset and setting.
- Keyboard shortcuts appear everywhere, with fuzzy search in every list.
- Selection-aware context actions.
- Every long list (Hierarchy, Project, Console, inspector arrays) is a virtualized list or tree view, so a scene of
  100,000 Objs scrolls as smoothly as one of 10.

**Core.**
- Undo and redo for every edit, as commands *(Inspector and handle edits implemented; see "What exists now")*.
- Play mode with state restore *(implemented; see "What exists now")*.
- **C# hot reload:** game scripts live in a collectible load context. On change, behaviour state is serialised, the
  context unloads, the new build loads, and the state is restored.

**Extending.** Custom inspectors, windows, gizmos and tools are written in C#, using the same Voxear UI.

### The Inspector and its attributes *(planned, in M3)*

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
A source generator turns the attributes into inspector metadata at compile time, so the editor reads no
reflection, and an analyzer reports a condition that names a missing member as a compile error.

**Advanced.** Custom drawers in C#, built with Voxear UI: `ValueDrawer<T>` for a type, and
`AttributeDrawer<TAttribute>` for your own attributes. Drawers chain as Odin's do, with each decorator wrapping the
next, so `[InfoBox]` above a custom drawer still shows.

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
- **Not yet:** the overlays' backdrop blur (they are translucent without it), and the concept's Voxel Brush panel,
  which is example content.

### The editor concept

The design target for M3's **look and layout**, whose style the design above now defines. The concept's content is only an example scene: its voxel
terrain, brush tools and asset names are placeholders, not planned features.

- **Top bar:**
  - the Voxear mark
  - workspace tabs (World, Animation, Profiling)
  - a command bar ("Search or run a command", Ctrl+K)
  - play, pause and step in the centre
  - the user's avatar, and an accent **Build** button that opens the Build window
- **Left rail.** Icons for the main areas (World, Layers, Code, Audio, Animation), with source control at the
  bottom.
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
  - A ready-made, themeable rebinding screen.
  - Conflict detection.
  - Per-player persistence.
  - Composites such as WASD into a Vector2.
- **Prompts are native.** A `<prompt action="Jump"/>` UI element, or `Input.Glyph("Jump")`, shows the correct icon
  for the current scheme, device family and layout (Xbox, PlayStation, Switch, Steam Deck, keyboard). Key names are
  localised and follow the user's keyboard layout, so `KeyQ` shows as "A" on AZERTY. The icon updates instantly
  when the scheme changes.
- Processors (deadzones, curves, invert), action phases, and polling and event APIs both work. Input is read on the
  main thread and sampled once per frame, plus per fixed step for physics.

## Localization

Every piece of text can be bound to a key. When the language changes, everything bound to it updates at once, with
no reload. This works like Unity's Localization package, but with the setup done for you.

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

**Mixer.** A Rust mixer on cpal with buses, effects, snapshots and ducking. 2D and 3D sources, Doppler, and
streaming for music.

**Spatial acoustics** come from Steam Audio:
- HRTF binaural rendering.
- **Occlusion and transmission:** sound is muffled through walls, and materials set absorption.
- **Reflections and reverb:** real-time or baked reverb probes, with echo from real geometry.
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

**Tools.** An audio debugger shows active voices, occlusion rays and paths in the viewport.

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

Modern terrain should be non-destructive, layered and GPU-built.

**Data model.**
- The world is a sparse grid of tiles, for example 256² plus a border, held in a GPU texture array behind a page
  table.
- A terrain is a **stack of layers**. Each layer is a *command*, not a raster: stamp, noise, brush stroke, erosion,
  spline or import, each with a blend op (add, max, min, set, lerp), opacity, mask, transform and bounds.
- Only **dirty tiles** are re-composited, in compute. Checkpoints every few layers mean editing layer *k* only
  re-runs layers *k* to *n*.
- Derived maps are computed alongside: normals, slope, curvature, flow accumulation, and min/max mips for culling,
  collision and ray queries.

**Erosion.**
- Erosion is a layer type: hydraulic (the pipe model) plus thermal, on the GPU, with the result cached.
- Stream-power erosion handles mountain-scale terrain.
- Erosion produces flow, sediment and debris masks that texturing and scattering can use.

**Texturing and scattering.**
- **Auto-material rules** paint by slope, height, curvature and flow. Defaults look good immediately (grass, rock,
  dirt, snow, triplanar cliffs).
- Biomes are planned.
- **Scatter layers** place vegetation and details from rules (density, slope, height, noise, masks), generated in
  compute with no CPU instance lists.

**Rendering.** A CDLOD quadtree with geomorphing, drawn through GPU culling and indirect draw. Runtime virtual
texturing comes later.

**Runtime editing.**
- A sparse **runtime delta layer** holds a height delta and a hole mask.
- Gameplay stamps (craters, digging, footprints) append commands, and only the affected tiles update.
- Collision updates asynchronously.
- Saves store only the deltas.

```csharp
Terrain.Stamp(Stamps.Crater, at: hit.Point, radius: 4f);                      // easy
terrain.Runtime.Apply(new HeightBrush(BlendOp.Subtract, mask, falloff));   // advanced
```

**MVP vs later.** The MVP is CDLOD, stamps and brushes, erosion, auto-material, scatter, holes, collision and
runtime stamps. Later come runtime virtual texturing, spline rivers, a biome graph, cave and overhang volumes, and
Gaea, World Creator and Houdini import.

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
- `vox doctor` checks the GPU, .NET and the shader compiler.
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
- **Frame debugger**, like Unity's, lighter than RenderDoc and always available:
  - Step through render graph passes inside the editor, with each pass's inputs and outputs.
  - Inspect any texture (mip, slice, channel, exposure) or buffer.
  - See culling results (instances and meshlets culled by frustum and occlusion, the LOD chosen) and pipeline state.
- **CPU and GPU profiler.**
  - Timeline and hierarchy views.
  - Cost per system, per job and per behaviour.
  - GPU time per render graph pass, from timestamp queries.
  - Managed allocations per frame.
  - **Tracy** is integrated for deep sessions: zones across Rust systems, jobs and C#
    (`using (Profiler.Zone("Pathfinding"))`), locks, plots and frame images.
- **Memory profiler.**
  - Every native allocation is tagged by subsystem: ECS chunks, Content, render, audio, physics and scripts.
  - GPU memory is reported per resource and category. The managed heap comes from the .NET runtime.
  - Snapshots can be diffed, for example before and after a level change to find leaks.
  - "Who holds this asset" shows the reference chain behind Content's refcounts.
  - A bundle view shows what is resident and why.
- **ECS inspector.**
  - Live archetypes, chunks and entities, with editable component values.
  - The system schedule, with timings and order.
  - A query tester.
- **Debug draw.**
  - `Debug.Line`, `Box`, `Sphere`, `Arrow` and `Text`, each with a duration, from C# and Rust.
  - Works in the game and the editor, depth-tested or drawn on top.
  - Compiled out of release builds.
- **Debug views.**
  - Albedo, normals, roughness and overdraw.
  - LOD and meshlet colouring, light complexity, GI voxels and shadow cascades.
  - Physics colliders, nav graphs, audio occlusion rays and AI paths.
- **Dev console.**
  - An in-game console with commands (`[Command]` on any C# static method) and console variables.
  - Also reachable through `vox`.
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

`vox build` runs the stages in order:
1. Content build (incremental).
2. Script build in Release, with optional NativeAOT.
3. Shader permutations.
4. Player packaging.

Each stage can also run on its own and caches its outputs. Builds are reproducible from the project folder alone.

### Build profiles

What to build is a **build profile**, like Unity 6's Build Profiles: a `.buildprofile` TOML asset under
`Settings/BuildProfiles/`. A project can have as many as it likes, such as `Windows Release`, `Linux Demo` and
`Steam Playtest`.

**Easy.** A new project comes with a Release profile and a Development profile for each platform. Pick one and
press Build, or run `vox build --profile "Windows Release"`.

**Advanced.** A profile only lists what differs from its base, the same way post-processing presets layer:

```toml
format = 1
base = "Windows Release"     # optional: inherit another profile
platform = "windows-x64"     # or "linux-x64"

[levels]
start = "Levels/Menu.level"
include = ["Levels/Demo/*.level"]   # default: every level the start level can reach

[development]
enabled = true
script_debugging = true
profiler_autoconnect = true
validation_layers = false
```

The full set of options a profile can carry:

| Group | Options |
| --- | --- |
| Platform | Target OS and architecture. Linux and Windows x64 now; arm64 later. |
| Levels | Start level and which levels ship. By default, what the start level can reach. Content is bundled from the levels' usage sets. |
| Development | Debug symbols, assertions, the in-game debug panels and console, profiler autoconnect, remote attach, script debugging, deep profiling, Vulkan validation. |
| Scripting | Backend: CoreCLR (fast builds, allows script mods) or NativeAOT (fast start-up, smaller, harder to reverse, no script mods). IL trimming level, and extra defines for C# (`#if DEMO`) and for shaders. |
| Content | Bundle compression (LZ4 for load speed, Zstd for size), texture quality caps, which quality tiers' shader variants ship, and asset stripping rules. |
| Player | Product and company name, icon, splash, default window mode and resolution, vsync, run in background, and crash reporter settings. |
| Version | Version string, an auto-incremented build number, and the git commit stamped into the player and the report. |
| Graphics | Defaults per quality tier, including the post-processing preset. |
| Mods | Whether mods load at all, which trust tiers are allowed, and whether to export the mod SDK alongside. |
| Store | Steam app and depot IDs, the branch to upload to, and whether to upload when the build succeeds. Other stores slot in through their providers. |
| Output | The folder and file naming pattern, clean or incremental, and Build and Run. |
| Hooks | Steps before and after each stage: a C# `IBuildStep` or a shell command, with the build's context as JSON. |

Switching the active profile is instant. Content is cached per platform, and Linux and Windows share desktop
texture formats, so there is no Unity-style reimport when changing platform.

**Builds for other platforms.** Player runtimes ship prebuilt in each engine install, per platform, as installable
modules that the launcher manages. Building a Windows player on Linux therefore compiles no Rust: it builds content
and scripts, then packages the Windows runtime. Script builds for CoreCLR are platform-neutral IL. NativeAOT can only
compile on the target OS, so the profile says so up front and offers a remote build agent once those exist.

### Builds run in their own process

The editor never builds inside itself. Pressing Build starts `vox build --profile <name> --progress json`, the same
command CI runs, and the Build window follows it. This means:
- The editor stays responsive, and you can keep working while it builds.
- A build that crashes cannot take the editor with it.
- A build behaves the same from the editor, the terminal and CI.
- Several profiles can build at once, one process each, sharing the content cache under a lock.

The process reports progress as one JSON object per line on stdout:

```json
{"event":"stage","stage":"content","index":1,"of":4}
{"event":"progress","stage":"content","done":120,"total":800,"item":"Textures/Rock.png"}
{"event":"log","level":"warn","message":"Texture is not a power of two","asset":"6f1c…"}
{"event":"finished","result":"succeeded","seconds":42.7,"report":"Builds/Windows Release/build-report.json"}
```

**The Build window** lists profiles on the left and the selected profile's settings on the right, like Unity 6. A
running build shows its stages with timings, a progress bar for the current stage, and the log, filterable by level.
Clicking a log entry selects the asset it names. Cancel asks the process to stop at the next safe point, which
leaves every cache consistent. It is killed only if it does not stop. Finishing posts a notification with the
result, the output folder and the report.

**The build report** comes with every build:
- Size by bundle, asset and type.
- Why each asset was included: the chain of references from a level.
- Stage timings, warnings and shader variant counts.
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

## Platforms and GPU support

**Targets.** Linux (Wayland and X11) and Windows 10/11 on x86-64. These are the only platforms for now.

**GPU support.** Any GPU with Vulkan 1.3 and the features listed above: NVIDIA (proprietary and NVK), AMD (RADV and
AMDVLK), and Intel (ANV and Windows drivers). `vox doctor` explains exactly what a GPU lacks.

**Optional tiers.** Mesh shaders, hardware ray tracing and DLSS/XeSS. Every feature has a fallback.

Other platforms (macOS, mobile, consoles, XR) are out of scope for now. The RHI still keeps platform code
contained, so adding one later does not mean reworking the engine.
