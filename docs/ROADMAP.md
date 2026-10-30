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

## M2 — Content, scenes and levels

- Asset database with GUIDs, `.meta` sidecars, importers and `Ref<T>`.
- Text scene format, then prefabs with nesting and variants.
- `Instantiate` and `AddBehaviour`, running Awake and OnEnable on the new batch before returning.
- Levels, scene diffing on level change, and Scene Persistence.
- Content build: usage-set bundling, a catalog, and incremental content hashing.
- Runtime loading, sync and async, with reference counting.
- Catalogs that layer (base plus overlays), so mods slot in at M11 without reworking loading.
- `vox content build`.

**Exit:** a two-level sample shares scenes between levels and loads only what each level needs.

## M3 — Editor shell and Voxear UI

- UI core:
  - taffy layout
  - parley and skrifa text with crisp hinted atlases
  - `.vui` markup and `.vss` styles
  - transitions
- OKLCH theme tokens.
- Docking, Hierarchy (with instance badges), Inspector, Project, Scene and Game views, and Console.
- Command palette (Ctrl+K), gizmos, selection, undo/redo, play mode, and C# hot reload.
- Debug tools:
  - the stats overlay and the profiler panel (with Tracy)
  - the frame debugger and the ECS inspector
  - the memory profiler with snapshots and diffs
  - debug draw and the dev console
- `vox new` with project templates and default setups, and versioned engine builds with a release manifest, which
  together are the contract the launcher needs.

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
- Audio mixer, 2D/3D sources, and Steam Audio occlusion, reflections and pathing.
- Jolt integration (bodies, joints, character controller, queries, events) and Box2D v3.

## M5 — Lighting

- Per-meshlet culling (frustum, cone, Hi-Z) on mesh shaders with a compute fallback, including shadow passes.
- Area and tube lights, IES profiles, sky and atmosphere.
- Volumetric fog.
- Virtual shadow maps.
- Post stack and upscalers.
- **Real-time voxel GI** (clipmaps, then injection, gathering and the radiance cache).
- Baked lightmapper and probes.

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
- Crash reporter, GPU crash diagnostics, remote attach for every debug panel, and documentation.
- Editor RPC for `vox`, and the hook for AI tooling.

## M13 — Virtualised geometry (Nanite-style)

M1 and M5 already lay the groundwork: meshlets, the visibility buffer and two-phase Hi-Z culling. This milestone
can move earlier if it becomes a priority.

- HLOD proxies per level and automatic octahedral impostors.
- A cluster DAG built at import.
- Continuous per-cluster LOD selected on the GPU.
- Cluster page streaming through Content.
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
