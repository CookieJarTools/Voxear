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

## M1 — Rendering core

- Render graph (barriers, transient aliasing, async compute) and bindless descriptors.
- Slang toolchain, with shader hot reload.
- glTF import, then meshlets and LOD generation.
- Rendering basics:
  - camera, depth and a PBR forward pass
  - directional, point and spot lights in clusters
  - cascaded shadow maps
- The first parts of GPU-driven culling.
- Debug views (wireframe, normals, overdraw) and GPU timestamps.

**Exit:** a glTF scene renders lit and shadowed at a solid frame rate, on Linux and Windows.

## M2 — Content, scenes and levels

- Asset database with GUIDs, `.meta` sidecars, importers and `Ref<T>`.
- Text scene format, then prefabs with nesting and variants.
- Levels, scene diffing on level change, and Scene Persistence.
- Content build: usage-set bundling, a catalog, and incremental content hashing.
- Runtime loading, sync and async, with reference counting.
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

**Exit:** you can build and play the Hello scene in the editor, and it looks like the concept.

## M4 — Input, audio and physics

- Input:
  - actions, schemes and auto-switching
  - the rebinding UI
  - prompt glyphs per scheme, device and locale
- Audio mixer, 2D/3D sources, and Steam Audio occlusion, reflections and pathing.
- Jolt integration (bodies, joints, character controller, queries, events) and Box2D v3.

## M5 — Lighting

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

## M11 — Ship it

- `vox build` end to end, NativeAOT players, and Windows CI.
- Profiler, frame debugger, memory tools and documentation.
- Editor RPC for `vox`, and the hook for AI tooling.

## Open questions

These are decisions that change design or naming. Each has a recommendation.

1. **Prefab name.** Keep "Prefab" (familiar), or rename it to fit Obj and Definition ("Template")? *Recommendation:
   keep Prefab.*
2. **What `Instantiate` returns.**
   - **Option A** *(recommended)*: the new batch's Awake and OnEnable run before `Instantiate` returns, so callers
     get an initialised object, as in Unity. Start still waits for the next flush.
   - **Option B**: everything is deferred to the next flush. This is simpler and stricter, but the object is not
     yet Awake when you configure it.
   - Today the engine does B; A needs a re-entrant lifecycle.
3. **Coordinate system.**
   - Left-handed Y-up, like Unity: easy migration and familiar.
   - Right-handed Y-up, like glTF and most tools: no import flips.
   - *Recommendation: right-handed Y-up in metres, converted transparently on import.*
4. **Scene file format.** A text format such as RON or TOML-like (diffable, mergeable), with a binary form generated
   for builds. *Recommendation: text in projects, binary in builds.*
5. **Engine licence.** Open source (MIT/Apache, or MPL), source-available, or private for now?
6. **Platforms after Linux and Windows.** macOS, Android, consoles, XR? This affects how early to abstract the RHI
   further.
