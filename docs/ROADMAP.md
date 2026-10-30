# Voxear roadmap

This is a multi-year engine. The order below gets to a usable editor early, then deepens each system. Each
milestone ends with something you can run. Design details for every system are in
[ARCHITECTURE.md](ARCHITECTURE.md).

## M0 — Foundation ✅ (done)

- **Cargo workspace.** Eight crates and a `vox` CLI.
- **Archetype ECS.** 16 KiB chunks, typed and runtime (C#) components, queries with `with`/`without`/`changed`
  filters, per-chunk slices, resources and an ordered schedule. 14 tests.
- **Ordered lifecycle.** Awake → OnEnable → Start batches, execution order, deferred enable/disable/destroy, and
  skipping of overrides a type does not have. 6 tests.
- **CoreCLR hosting.** Hosted through `hostfxr` directly.
  - `Voxear.Core` is the C# API: `Obj`, `Behaviour`, `Definition`, `IInstance`/`Instance<T>`, `[ExecutionOrder]`,
    `Time` and `Log`.
  - Game scripts load into a collectible context, ready for hot reload.
- **Vulkan 1.3 device.** GPU selection and capability report, swapchain with resize handling, frame pacing with two
  frames in flight, and a first pipeline using dynamic rendering.
- **`vox` CLI.** `vox doctor`, `vox run` (windowed or `--headless`), and the `samples/Hello` project.

## M1 — Rendering core ✅ (done)

Done so far:
- **Render graph** with automatic barriers, debug labels and per-pass GPU timers.
- **Bindless resources:** textures and samplers by index, and buffers by device address.
- **Slang shaders** compiled at build time.
- **GPU-driven drawing:** GPU frustum culling writes indirect draws, and all meshes share one vertex and index
  buffer.
- **Lighting:** a camera with reverse-Z depth, a PBR forward pass, a sun plus point and spot lights, an analytic sky,
  and ACES tonemapping.
- **glTF import:** mirrored to Unity's coordinates, with textures and mips.
- **Hierarchy:** parent/child transforms.
- **Rendering modes:** headless offscreen rendering (`vox render`), and RenderDoc capture (F12 or `--renderdoc`).
- **Two-phase Hi-Z occlusion culling.**
  - The early pass draws what was visible last frame.
  - A depth pyramid is built from that depth.
  - The late pass draws whatever newly became visible, and records visibility for the next frame.
  - GPU counters report what was culled, and why.
- **Automatic LODs:** generated on import with meshoptimizer, and chosen per instance on the GPU by screen-space
  error.
- **Sun shadows:** four cascades fitted to stable spheres and snapped to texels, rendered from GPU-culled draw
  lists, with 3×3 hardware PCF.
- **Debug views:** LOD colours, shadow cascades, light complexity and normals (`--debug-view`, or F2 to F5 in the
  window).
- **Samples:** `samples/Stress` (1,600 spheres and a wall) for culling and LOD, and `samples/Lights` (256 point lights
  at night) for clustered lighting.
- **Clustered lighting:** a 16×9×24 froxel grid whose light lists are built on the GPU each frame. 256 lights shade a
  1280×720 frame in about 0.3 ms on an RTX 3070.
- **Dithered LOD cross-fades:** when an instance changes LOD, both LODs draw for a quarter of a second with
  complementary dither patterns.
- **Shader hot reload:** in a source checkout, saving a `.slang` file recompiles it and swaps pipelines while running.
  Errors are logged and the previous shaders keep running.
- **Scene-view fly camera** in the window, with Unity's controls.
- **GPU integration tests** for occlusion, clustering and LOD fading, rendering offscreen. They skip on machines
  without a GPU.

Moved to M5 (Lighting), where they belong with the rest of the shadow and geometry work:
- Shadows for point and spot lights.
- Meshlet-level culling.

The original M1 scope:

- Render graph (barriers, transient aliasing, async compute) and bindless descriptors.
- Slang toolchain, with shader hot reload.
- glTF import, then meshlets and LOD generation.
- Rendering basics:
  - camera, depth and a PBR forward pass
  - directional, point and spot lights in clusters
  - cascaded shadow maps
- **GPU culling:** frustum culling plus two-phase Hi-Z occlusion culling per instance, with no CPU readback.
- **Built-in LOD:** LODs generated on import, selected on the GPU by screen-space error, with dithered
  cross-fades.
- Debug views (wireframe, normals, overdraw), GPU timestamps, and Vulkan debug names on every pass and resource.
- One-button RenderDoc capture through its in-app API.

**Exit:** a glTF scene renders lit and shadowed at a solid frame rate, on Linux and Windows.

## M2 — Content, scenes and levels ✅ (done)

Done so far:
- **Asset database** (`voxear_assets`): GUIDs in `.meta` sidecars, moves and copies handled, asset references with
  parts (`guid#mesh0.0`), and `vox assets`.
- **Scenes:** the `.scene` TOML format, with loading and saving. A round trip is tested.
- **Content:** source assets turned into GPU meshes and materials, one import each. Supports `.material` files and
  inline materials, and remembers sources for saving.
- **Levels:** `.level` files, scene diffing (shared scenes stay loaded), `reload`, and Scene Persistence with no
  duplicates on reload.
- **Destroying Objs:** deferred destruction (OnDisable, then OnDestroy, then removal at the end of the frame;
  hidden from rendering at once).
- **From C#:** `Level.Load`.
- **Samples:** `start_level` in `voxear.toml`, replacing the old `[[objs]]` list. Hello is now a two-level day and
  night sample sharing its environment scene, which meets the exit criterion's scene-sharing half.
- **Post-processing, the Voxear Look** (pulled forward from M5):
  - motion vectors
  - TAA, FXAA and contrast-adaptive sharpening
  - bloom and auto exposure
  - AgX, ACES, Khronos PBR Neutral and Reinhard tonemapping, with white balance and lift/gamma/gain grading
  - vignette, height fog, depth of field, motion blur, chromatic aberration, grain and dithering
  - `voxear`, `neutral` and `cinematic` presets, set per scene in `[post]`, and `vox render --post`

- **Prefabs:** `.prefab` files, instances in scenes with per-field overrides, nesting and variants, and instances
  saved back as instances.
- **Instantiate from Rust and C#:** `Prefab.Load` and `Instantiate`, `AddBehaviour`, `GetBehaviour` and
  `Obj.Destroy`. Awake and OnEnable run before Instantiate and AddBehaviour return. Runtime Objs unload with the
  active scene. Hello's night lamps are prefab instances and a variant, and its fireflies are instantiated at
  runtime.

- **Behaviour fields** set from scenes and prefabs before Awake (`[obj.fields."Type"]`, `[SerializeField]`),
  layering through instances and overrides. Covers numbers, vectors, rotations, enums, lists, `Prefab` and the lazy
  `Ref<T>`, with warnings that name the field when a value does not fit. `Obj` fields reference Objs in the same
  file as `{ obj = 3 }`, follow each prefab instance to its own copies, and save back as ids or paths.

- **Math helpers** for scripts: Unity's Vector3, Vector2, Quaternion, Mathf and Random helpers on the
  System.Numerics types, through C# 14 extension members. Also Obj world position and rotation, the
  Forward/Right/Up directions and `LookAt`, all tested in `managed/Voxear.Core.Tests`.

- **Content build:**
  - `vox content build` walks the dependency graph from levels, prefabs and field-named assets, and groups assets by
    usage set into LZ4 bundles.
  - The bundles are named by content hash, with a readable catalog.
  - Every read goes through an `AssetReader`, so `vox run --bundles` plays from built content. Hello shows each level
    opening only the bundles it needs, which meets the exit criterion.

- **Async loading:**
  - `Level.LoadAsync` with progress, `IsReady` and `AllowActivation`, and `Prefab.LoadAsync`, both awaitable.
  - A worker thread reads files and prepares models: glTF parsing, image decoding and LOD building. The main thread
    uploads them within a per-frame budget.
  - A main-thread `SynchronizationContext` brings every await back to the frame. Hello switches levels with
    `await Level.LoadAsync`.

- **Unloading:** after every level change, everything no live Obj or prefab in use needs is freed: models, meshes,
  materials, textures, prefab data and open bundles. The GPU buffers use a range allocator, with frees deferred past
  frames in flight. Developer controls are `Content.UnloadUnused()`, `Content.Usage` and `prefab.Keep()` /
  `Release()`.

- **Fields and content roots:** nested `[Serializable]` types and behaviour or interface references in fields, and
  `[content]` include and exclude patterns for the build.

- **Cooked models:** the content build stores models ready for the GPU, with meshes, LODs, decoded textures and
  materials, so loading needs no import work. A cache keyed on input hashes means unchanged models are never cooked
  twice.

- **Compressed textures:** cooking compresses colour textures to BC7 and normal maps to BC5, with mips built on the
  CPU in linear space. The encoders are Voxear's own, and the GPU checks BC support at startup.

- **Bundle sizing and layered catalogs:**
  - Large groups split, and tiny bundles merge into one their roots open anyway (Hello goes from 7 bundles to 3).
  - `Content::mount` and `--overlay` layer built content over the game, replacing assets by GUID and adding new
    ones. This is the base mods build on in M11.

- **`[ContentRoot]`:** code that loads by path names what it loads, so the build ships it even past `exclude`,
  and warns about patterns that match nothing.

- **Async IO:** batched reads through io_uring on Linux and IoRing on Windows 11, with a plain fallback.
  Background loads read in waves, one batch per step down the dependency graph.

- **GPU decompression:** cooked models carry their GPU data as one GDeflate payload in 64 KB tiles. The GPU
  decompresses it (`VK_NV_memory_decompression`) and uploads the whole model in one submission; without that
  extension the loading thread decompresses it.

**Exit:** a two-level sample shares scenes between levels and loads only what each level needs.

## M3 — Editor shell and Voxear UI

Done so far:
- **Voxear UI foundation** (`voxear_ui`):
  - a retained tree of panels and labels with taffy flexbox layout
  - parley text from system fonts, rasterised hinted by swash at the physical size into a glyph atlas
  - SDF rounded, bordered quads and glyphs, drawn in one pass after post-processing with linear blending
  - tests without a GPU, and one that checks the drawn pixels
- **Markup and style sheets:** `.vui` documents and `.vss` sheets in the spirit of UXML and USS: USS's
  selectors, the CSS cascade with inherited text and `var()` theme tokens, `oklch()` colours, and warnings that
  name the file and line.
- **Pointer:** `:hover`, `:active`, clicks and the wheel.
- **Controls:** ScrollView with clipping and a floating scrollbar, and virtualized ListView and TreeView, which
  keep elements only for the rows on screen. A list of 100,000 items makes 11 rows.
- **Stats overlay**, the first thing built with it: F1 in a game window, `--stats` on `vox run` and `vox render`,
  written as `StatsOverlay.vui` and `.vss`. Clicking its header folds it.

Still to do in M3:
- UI core:
  - transitions
  - keyboard focus and text input
  - toggles, sliders, foldouts, dropdowns, tabs and split views
  - grid layout properties
- **Inspector with Odin-style attributes** (see "The Inspector and its attributes" in the architecture):
  grouping, conditions, value controls, validation, buttons and table lists, from source-generated metadata, plus
  custom drawers in C#.
- OKLCH theme tokens.
- Docking, Hierarchy (with instance badges), Inspector, Project, Scene and Game views, and Console.
- Command palette (Ctrl+K), gizmos, selection, undo/redo, play mode, and C# hot reload.
- Debug tools:
  - the stats overlay's frame-time graph, and the profiler panel (with Tracy)
  - the frame debugger and the ECS inspector
  - the memory profiler with snapshots and diffs
  - debug draw and the dev console
- **C# ECS API**, the advanced path beside Behaviours (see "ECS and the Obj model" in the architecture):
  - C# components as `unmanaged` structs, registered by name and layout, living in the same chunk memory as Rust
    components
  - `SystemBase` systems placed in schedule stages, with queries over chunks that hand out `Span<T>` reads and
    writes with no copies, plus `With`, `Without` and `Changed` filters
  - entity commands from C#: create, destroy, and add or remove components, applied at sync points
  - a bridge to Objs (`obj.Add<Spin>()`, `obj.Get<Spin>()`), so Behaviours and systems can mix
  - components written in scenes and prefabs, and shown in the inspector
  - Later: parallel jobs over chunks.
- `vox new` with project templates and default setups, and versioned engine builds with a release manifest, which
  together are the contract the launcher needs.
- **Build profiles and the Build window:**
  - `.buildprofile` assets that inherit from a base, with a Release and a Development profile per platform in new
    projects
  - levels, development options, scripting defines, compression, player settings and versioning
  - `vox build --profile` for the host platform, with CoreCLR players
  - builds in their own process, reporting progress as JSON lines
  - the Build window: live stages, timings, the log, cancel and notifications

**Exit:** you can build and play the Hello scene in the editor, and it looks like the concept.

## M4 — Input, localization, audio and physics

- Input:
  - actions, schemes and auto-switching
  - the rebinding UI
  - prompt glyphs per scheme, device and locale
- **Localization:**
  - string and asset tables
  - ICU smart strings (plurals, gender, number and date formatting)
  - binding through `@key` in UI markup, `LocalizedString` fields, `LocalizedText`, and observable `Localized(key)`
  - CSV import from a file or URL with a diff preview, and CSV export
  - the Localization Tables editor window, with pseudo-localisation
  - automatic language detection (saved choice, then store, then OS) with fallback chains
  - RTL mirroring and per-locale fonts
- Audio mixer, 2D/3D sources, and Steam Audio occlusion, reflections and pathing. Ray-traced audio follows in M5,
  once the ray-tracing acceleration structures exist.
- Jolt integration (bodies, joints, character controller, queries, events) and Box2D v3.

## M5 — Lighting

- Per-meshlet culling (frustum, cone, Hi-Z) on mesh shaders with a compute fallback, including shadow passes.
- Area and tube lights, IES profiles, sky and atmosphere.
- Volumetric fog.
- Virtual shadow maps.
- Upscalers (FSR built in, DLSS and XeSS optional), colour grading LUTs, and local post volumes.
- **Real-time voxel GI** (clipmaps, then injection, gathering and the radiance cache).
- Baked lightmapper and probes.
- **Ray-traced audio** as an optional setting: GPU rays against the GI's acceleration structures, fed to Steam
  Audio, with baked reverb probes as the fallback.
- **HDR colour, end to end.** Rendering is HDR already: a float target, emissive and lights above 1, exposure and
  tonemapping. Still to come:
  - a C# `Color` type, with HDR intensity for fields such as `[ColorUsage(hdr: true)]`
  - HDR textures (`.exr` and `.hdr`) imported as half floats and cooked to BC6H, for skies, light probes and
    emissive maps
  - HDR display output: HDR10 and scRGB swapchains with paper-white and peak-brightness settings, and tonemapping
    that targets the display

## M6 — Animation

- GPU skinning, blend shapes, state machines, blend trees, layers and root motion.
- Motion matching: the feature database, then search, then inertialisation, then GPU crowd search.
- IK.

## M7 — Materials and particles

- Shader Graph with previews and sub-graphs, plus material variants.
- Particles:
  - the IR compiler and the stateless fast path
  - the preset gallery
  - stack mode, then graph mode
  - depth collision, ribbons and flipbook baking
  - the particle debugger

## M8 — Terrain

- CDLOD rendering.
- Layer stack and dirty-tile compositing.
- Stamps and brushes, then GPU erosion.
- Auto-material and scatter layers.
- Holes, collision and the runtime delta layer with its API.

## M9 — Navigation, cameras and Timeline

- Navigation:
  - grid, layered grid, recast and point graphs
  - async paths, modifiers and runtime graph updates
  - RVO avoidance
- Camera Rig presets, the composable rig API, blends and shakes.
- Timeline.

## M10 — 2D and networking

- Sprites and atlases, tilemaps with rule tiles, 2D lights, and the pixel-perfect camera.
- Replication, RPCs, prediction and rollback, interpolation, the network simulator, and multi-instance play mode.

## M11 — Modding and platform services

- **Platform API.** A provider interface with capability checks, an offline provider, and Steam: achievements,
  stats, leaderboards, cloud saves, rich presence, the overlay, Steam Input, lobbies and Steam Networking Sockets.
- **Content mods.** Layered catalogs, overridable-asset rules, Definition patches and conflict detection.
- **Script mods.** The `[ModApi]` surface, the allowlist scan, per-mod load contexts, fault isolation and the
  watchdog.
- **Mod packaging.** `mod.toml` manifests, dependency and version resolution, load order, signing, and the consent
  prompt.
- **Distribution and tools.**
  - The built-in mod manager screen.
  - Steam Workshop as a mod source and publish target.
  - Multiplayer mod-set matching.
  - Mod SDK export and editor mod mode.
  - `vox mod` and `vox steam upload`.

## M12 — Ship it

- `vox build` end to end, NativeAOT players, and Windows CI.
- **Builds, advanced:**
  - other-platform builds from prebuilt player modules installed by the launcher
  - the build report with size breakdowns, inclusion chains and diffs against the last build
  - a build queue, with several profiles building at once
  - pre- and post-stage hooks (`IBuildStep` or shell commands)
  - store upload straight from a profile (Steam first)
  - remote build agents for NativeAOT on other operating systems
- Crash reporter, GPU crash diagnostics, remote attach for every debug panel, and documentation.
- Editor RPC for `vox`, and the hook for AI tooling.

## M13 — Virtualised geometry (Nanite-style)

M1 and M5 already lay the groundwork: meshlets, the visibility buffer and two-phase Hi-Z culling. This milestone
can move earlier if it becomes a priority.

- HLOD proxies per level and automatic octahedral impostors.
- A cluster DAG built at import.
- Continuous per-cluster LOD selected on the GPU.
- Cluster page streaming through Content, with uploads on a queue of their own so the main thread never waits.
- Compute rasterisation for tiny triangles.

## Separate repository: Voxear Launcher

The launcher, the equivalent of Unity Hub, lives in its own repository. It covers:
- installing engine versions side by side
- updating itself
- the project list
- the new-project flow with templates and default setups

It relies on the engine contract described under "Versions, installs and the launcher" in
[ARCHITECTURE.md](ARCHITECTURE.md).

## Decisions

Settled on 2026-10-30:

- **Prefab** keeps its name.
- **`Instantiate` works like Unity's:** Awake and OnEnable run on the new batch before it returns, and Start waits
  for the next flush.
- **Coordinates match Unity:** left-handed, Y up, Z forward, metres.
- **Licence:** PolyForm Shield 1.0.0.
- **Platforms:** Linux and Windows only for now.

## Open questions

1. **Scene file format.** A text format such as RON or TOML-like (diffable, mergeable), with a binary form generated
   for builds. *Recommendation: text in projects, binary in builds.*
