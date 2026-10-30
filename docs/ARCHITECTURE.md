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
| Cinemachine | **Camera Rigs** | Simple by default. |
| Timeline | **Timeline** | |
| Shuriken / VFX Graph | **Particles** | One system with three levels of depth. |

## Layers

```
 ┌──────────────── Editor ────────────────┐ ┌─── vox CLI ───┐ ┌─── Player ───┐
 │ panels, gizmos, undo, play mode, C# ext │ │ build, run,   │ │ shipped game │
 └─────────────────────────────────────────┘ │ inspect, RPC  │ └──────────────┘
 ┌──────────────────────────── voxear_engine: frame loop ─────────────────────┐
 │  Scenes/Levels · Content · Input · Audio · Animation · Physics · Navigation │
 │  Particles · Terrain · UI · Cameras · Timeline · Networking                 │
 ├───────────────────────── voxear_render: render graph ──────────────────────┤
 │  GPU-driven geometry · lighting · shadows · voxel GI · fog · post · text   │
 ├────────────┬──────────────┬───────────────┬────────────────┬───────────────┤
 │ voxear_rhi │ voxear_ecs   │ voxear_scene  │ voxear_scripting│ voxear_core   │
 │ Vulkan/ash │ archetypes   │ Obj+lifecycle │ CoreCLR host    │ math, time    │
 └────────────┴──────────────┴───────────────┴────────────────┴───────────────┘
```

### Repository layout

```
crates/            Rust engine (one crate per system)
  voxear_core      math (glam), time, logging
  voxear_ecs       archetype ECS, queries, schedule
  voxear_scene     Obj components, behaviour lifecycle
  voxear_rhi       Vulkan device, swapchain, frame pacing
  voxear_render    renderer and passes, shaders/
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
| UI | **Our own retained UI** on taffy, parley, skrifa and cssparser, rendered by our Vulkan renderer | See [UI](#ui-voxear-ui). |
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

// Advanced: a C# system over chunk spans, no per-object calls (planned API).
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

## Scenes, Levels and Scene Persistence

- A **Scene** is a text asset (diff- and merge-friendly) holding Objs and their components.
- A **Level** is a Definition listing the Scenes it needs, plus optional "additive on demand" scenes.
- `Levels.Load(level)` diffs the scenes loaded now against the scenes the level needs:
  - Scenes shared between the two levels **stay loaded** unless marked `ReloadOnLevelChange`.
  - Missing scenes load.
  - Scenes the new level does not need unload, and their content refcounts release.
- Sync and async are a choice per call:
  - `Levels.Load(level)` loads synchronously.
  - `await Levels.LoadAsync(level, progress)` streams, and can activate behind a loading screen.
  - `LoadMode.Preload` loads but keeps the level inactive until `Activate()`.
- **Scene Persistence:** Objs with `ScenePersistence` move to the persistent session scene and survive every level
  change, like `DontDestroyOnLoad`. `obj.Persist()` is the easy form.
- Loading a scene is one lifecycle batch, so ordering guarantee 2 holds across the whole scene.

## Content (assets, Bundles, loading)

This is the only way to load assets. It replaces Addressables and fixes the build pain by making bundling automatic.

**Authoring.** Every asset has a stable GUID (stored in a sidecar `.meta`). A field of type `Ref<Mesh>`,
`Ref<Material>`, `Ref<Level>` and so on is all a developer writes. The editor's object picker fills it in.

```csharp
public Ref<Prefab> Enemy;

var enemy = Enemy.Instantiate(at: spawnPoint);          // easy: loads its bundle if needed
var mesh = await MeshRef.LoadAsync();                     // async
using var handle = Content.Load(MeshRef, Priority.High);  // advanced: explicit lifetime and priority
```

**Content build.** Runs automatically before a player build, or alone with `vox content build`.

1. Walk the dependency graph from every root: levels, scenes, and anything loaded by `Ref` from code. Static analysis
   finds `Ref<T>` fields, and `[ContentRoot]` covers dynamic cases.
2. For each asset, compute its **usage set**: the roots that can reach it.
3. **Group assets by identical usage set.** Each group is a bundle. Loading any root then loads exactly the bundles
   whose usage set contains it, and nothing it does not need. Shared assets between levels end up in their own
   bundles automatically.
4. Refine: split groups over a size cap, and keep large streamable data such as texture mip tails and audio streams
   in separate bundles so they stream independently. Merge tiny bundles only when per-bundle overhead would dominate
   (the threshold is configurable).
5. The build is incremental and content-hashed: unchanged inputs reuse cached outputs. It writes a **catalog**
   mapping asset GUID to bundle, offset and size, plus a bundle dependency list.

**Runtime.**
- Loading a `Ref` resolves its bundle from the catalog, loads that bundle and its dependencies (reference counted),
  and returns the asset.
- IO is async: io_uring on Linux and IoRing/overlapped IO on Windows. GPU data uploads on a transfer queue, with
  GPU decompression where available.
- When the last reference goes away, the bundle unloads.
- In the editor the same API reads straight from the asset database, so no build is ever needed to press Play.

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

**Post-processing.** TAA, then the upscalers (FSR built in, DLSS and XeSS optional), bloom, auto exposure, depth of
field, motion blur, colour grading LUTs, and AgX or ACES tonemapping.

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

## UI (Voxear UI)

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
- **skrifa** for hinted glyph outlines (it is Chrome's font backend on Linux)
- **cssparser + selectors** for the stylesheet engine

What we write is the cascade, tokens, transitions, widgets, the renderer, and the C# binding layer.

**Authoring.**
- Markup (`.vui`) and stylesheets (`.vss`), in the spirit of UXML and USS, with design tokens as CSS variables.
- Transitions and animations.
- Data binding to C#, using source generators instead of reflection.
- A live-reloading UI builder in the editor.

**Crisp text** uses two paths, the same split Chrome uses:
- **Screen-space and editor text** is rasterised at the exact physical pixel size with light hinting. It goes into an
  atlas keyed by font, size and one of 4 sub-pixel x positions, with the baseline snapped to whole pixels. A
  contrast and gamma table tuned for light-on-dark text is applied. This is what makes small editor text sharp.
- **World-space and large or transformed text** uses MSDF, with an option for Slug-style curve rendering (now patent
  free). It stays sharp at any distance and angle. The renderer switches paths on scale and perspective.

**Rendering.** Shapes are instanced SDF quads, so rounded rectangles, borders, shadows and gradients stay crisp at
any DPI. World-space panels render to a mipmapped texture sized to their on-screen pixel density, then draw as
surfaces in 3D.

**The editor is built with Voxear UI**, so every editor improvement also improves game UI.

## Editor

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

**Speed.**
- A **command palette** on Ctrl+K reaches every action, asset and setting.
- Keyboard shortcuts appear everywhere, with fuzzy search in every list.
- Selection-aware context actions.

**Core.**
- Undo and redo for every edit, as commands.
- Play mode with state restore.
- **C# hot reload:** game scripts live in a collectible load context. On change, behaviour state is serialised, the
  context unloads, the new build loads, and the state is restored.

**Extending.** Custom inspectors, windows, gizmos and tools are written in C#, using the same Voxear UI.

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

A GPU ray-traced propagation path can follow later.

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

**Planned.**
- Project: `vox new`, `vox open`.
- Building: `vox build [--platform]`, `vox content build`, `vox scripts build`.
- Testing: `vox test`.
- Scenes: `vox scene list/inspect`.
- `vox import`, `vox profile`.

**Editor RPC.** A running editor exposes a local JSON-RPC socket, so `vox` can drive it: select, create and modify
Objs, run play mode, capture the viewport. AI tooling can later sit on the same RPC, for example as an MCP server,
without changing the engine.

## Build pipeline

`vox build` runs the stages in order:
1. Content build (incremental).
2. Script build in Release, with optional NativeAOT.
3. Shader permutations.
4. Player packaging.

Each stage can also run on its own and caches its outputs. Builds are reproducible from the project folder alone.

## Platforms and GPU support

**Targets.** Linux (Wayland and X11) and Windows 10/11 on x86-64. These are the only platforms for now.

**GPU support.** Any GPU with Vulkan 1.3 and the features listed above: NVIDIA (proprietary and NVK), AMD (RADV and
AMDVLK), and Intel (ANV and Windows drivers). `vox doctor` explains exactly what a GPU lacks.

**Optional tiers.** Mesh shaders, hardware ray tracing and DLSS/XeSS. Every feature has a fallback.

Other platforms (macOS, mobile, consoles, XR) are out of scope for now. The RHI still keeps platform code
contained, so adding one later does not mean reworking the engine.
