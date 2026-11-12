# Voxear roadmap

This is a multi-year engine. The order below gets to a usable editor early, then deepens each system. Each
milestone ends with something you can run. Design details for every system are in
[ARCHITECTURE.md](ARCHITECTURE.md).

The README's picture of it is drawn by `docs/images/draw_roadmap.py`; when a milestone changes here, change its
list too and run it again.

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
- **FBX import** through ufbx, into the same prepared model as glTF: units and axes baked into the vertices, Phong
  materials read as PBR, and textures found beside the file when the path it names does not hold.
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
- **Samples:** `samples/Stress` (1,600 spheres and a wall) for culling and LOD, `samples/Lights` (256 point lights
  at night) for clustered lighting, and `samples/Effects` (Particle Systems in a wind, a million sparks among
  them).
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
  cross-fades. A model's own LODs (`Rock_LOD0`, `Rock_LOD1`…) are used in their place, unless its import
  settings turn off Use its own LODs.
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

- **Today's C# in what a game's author reads:** the new-script and new-project templates, Hello's scripts and the
  manual's examples have a namespace for the whole file on one line (`namespace MyGame;`) and collection
  expressions (`private Obj[] lamps = [];`), as a game written now would.

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

## M3 — Editor shell and Voxear UI (continuous)

The editor is never finished, as Unity's is not: M3 stays open beside every other milestone, each of which brings
windows and tools of its own, and it takes what users find missing or falling short as they go. The lists below
say what is done and what comes next.

Done so far:
- **Voxear UI foundation** (`voxear_ui`):
  - a retained tree of panels and labels with taffy flexbox layout
  - parley text from system fonts, rasterised hinted by swash at the physical size into a glyph atlas
  - SDF rounded, bordered quads and glyphs, drawn in one pass after post-processing with linear blending
  - tests without a GPU, and one that checks the drawn pixels
- **Markup and style sheets:** `.vui` documents and `.vss` sheets in the spirit of UXML and USS: USS's
  selectors, the CSS cascade with inherited text and `var()` theme tokens, `oklch()` colours, and warnings that
  name the file and line. The cascade styles again only what changed, and finds rules by name, class and type,
  so the editor's idle frame spends nothing on styles where it spent 8 ms.
- **Pointer:** `:hover`, `:active`, clicks and the wheel.
- **Controls:** ScrollView with clipping and a floating scrollbar, and virtualized ListView and TreeView, which
  keep elements only for the rows on screen. A list of 100,000 items makes 11 rows.
- **Input controls and the keyboard:** TextField (on parley's editor: selection, word movement, clipboard, input
  methods), Toggle, Slider and Foldout, keyboard focus with Tab order, `text-align`, and a default theme in the
  editor concept's look. Window events reach the UI first through `ui_input`, which wraps the system clipboard.
- **Layout and motion:** CSS grid through taffy's own parser, transitions with CSS's easing curves, and
  SplitView, Tabs and Dropdown (with a popup layer above everything).
- **Docking:** areas of tabbed panels in split panes, rearranged by dragging tabs into an area or onto its edge,
  with a drop preview; layouts read and write as text for workspace presets.
- **Stats overlay**, the first thing built with it: F1 in a game window, `--stats` on `vox run` and `vox render`,
  written as `StatsOverlay.vui` and `.vss`. Clicking its header folds it.

- **The editor, first slice** (`vox editor`): edit mode without scripts, the concept's layout with the top bar,
  docked Hierarchy, Project, Scene, Console and Inspector, and the status bar. The Scene view renders into its
  panel through the editor's own fly camera, and the Inspector edits the selected Obj's name and Transform.
- **The editor's design:** the default theme and the editor follow the design's tokens, type and shapes (the
  Graphite + Abyss spec, then the concept's colours and a more compact density):
  the top bar with the mark, workspaces, command bar and play pill, an icon rail, 40px panel headers with their
  own buttons, 28px rows with an accent bar on the selected one, viewport overlays (tool strip, stats chip and an
  axis gizmo that turns with the camera), the Inspector's header, sections and coloured axis fields, and the
  status bar. Voxear UI gained line icons, `box-shadow`, dashed borders, `line-height`, shapes, a switch and
  checkbox, segmented controls, slider values, double clicks, and splits that keep the second pane's size or the
  first's share.
- **A Unity-style Project panel:** folders as a tree on the left, and the open folder's contents on the right as
  tiles or, when narrow, as rows with sizes, with a breadcrumb; double-clicking a folder opens it.
- **Play mode with state restore:** Play copies the edited level into a fresh engine with the scripts (built and
  started on the first Play, then kept, since .NET starts once per process) and shows the Game view; Stop
  destroys the played world and brings the edited level back as it was. Pause and Step, and the selection
  follows between the two.
- **VoxDocs, first slice** (`crates/voxear_docs`, after the CookieJarTools Cookbook, and called the Cookbook at
  first): Markdown pages in the project's `Docs` folder, read and written in a VoxDocs tab, with the page tree, search, back and forward,
  links to pages, headings, files and the web, missing pages written from their links, checklists that save,
  callouts, tables and code. Voxear UI gained rich text with clickable links, and `white-space: nowrap`.
- **LayerCake as level loading** (after the CookieJarTools scene switcher): scenes in a level marked `active`,
  `required = false` or with a `weight`, levels that stand on a base level, background changes with weighted
  progress, held activation, a least time and options to keep the rest or the active scene, `Level.LoadAsync` with
  `LevelLoadOptions`, `Ready`, `Activate`, a throwing await and static switch events, and a Levels panel in the
  editor that opens, plays and switches levels.
- **Saving and undo:** Ctrl+S writes the open level's changed scenes back to their files, round-tripping Hello's
  unchanged; undo and redo for Inspector edits; an unsaved marker, and a second click before a level change
  discards edits.
- **Scene view tools:** click an Obj to select it; move, rotate and scale handles driven by the tool strip (and Q,
  W, E, R), Pivot or Center, Global or Local, snapping with Ctrl, and each drag one step to undo.
- **Editing Objs:** context menus and dragging rows in Voxear UI; the Hierarchy makes, duplicates, deletes and
  reparents Objs, with undo, keeping each in its scene with its id; values marked "Keep value after play" in the
  Inspector come back into the edited level when play stops.
- **Components in the Inspector:** Mesh Renderer, Light and Camera values edited in place; script fields described
  from the scripts' build by a .NET tool (`Voxear.ScriptInfo`), rebuilt in the background when a script changes,
  with their starting values; references picked in a searchable object picker or dragged from the Hierarchy and
  the Project, colours in a colour picker (HDR, with intensity in stops, for emission), lists
  with items added and taken out, and nested
  `[Serializable]` tables; Add Component and Remove Component; all of it undoable, and component values keepable
  after play.
- **Switching Objs and components off**, as Unity's `SetActive` and `enabled`: the Inspector's tick by an Obj's
  name switches it and everything under it off (its behaviours get OnDisable and OnEnable, and the Hierarchy dims
  it), and each component's tick switches that one off, its values kept and still editable. Rendering skips what is
  off (lights, meshes, models, foliage, wind, seasons, post volumes, cameras, LOD Groups), and so do physics (3D
  and 2D colliders, characters, vehicles, wheels), sound (sources, listeners, acoustic walls) and the Seasons
  clock. A behaviour's own tick gives it OnDisable and stops its updates, its C# `Enabled` following it. Undoable;
  saved in scenes and prefabs as `active = false` and `enabled = { light = false, "Game.Flicker" = false }`, left
  out when on; `Obj.Active`, `ActiveInHierarchy`, `IsEnabled`/`SetEnabled(EngineComponent)` and `Enabled` on the
  component handles in C# (native API 61).
  - [ ] Objs that are off left out of Scene view picking, and their gizmos hidden, as in Unity
- **Command palette** (Ctrl+K): commands with their shortcuts, Objs, levels, assets, settings and VoxDocs pages,
  matched fuzzily, run from the keyboard or with a click, with what ran recently first. Scripts add commands with
  `[Command]` (run in edit mode against the edited level, undoable, with their own keys), and Rust code with
  `Editor::add_command`.
- **Settings:** the Scene view's and play mode's settings in the rail's Settings drawer, a tab per group with a
  search over all of them, and in the palette, kept per project with the palette's recent items.
- **Hierarchy order:** scene headers, Objs in their scene's order (saved as shown), dragging before, after or into a
  row or onto a scene, undo putting Objs back at their places, `IInstance` badges and tinted prefab instances.
- **The Console, after Sifter:** one line per row in columns, categories nobody declared (a `[Tag]`, the scripts'
  namespace, the engine's crate, the compiler) in a Categories menu, a search with words, `tag:`, `-tag:` and `/regex/`, level
  toggles with counts, Collapse, Clear, Error Pause and muting; the selected line's stack coloured part by part, the
  engine's frames folded under My code only, each frame opening its file at its line in the code editor the
  settings name, and a line about an Obj selecting it. Scripts' lines carry their .NET stack and Obj.
- **Engine installs for the launcher:** an install finds its own files (`VOXEAR_HOME`, or `engine.toml` above
  `bin/`), game scripts build against the engine running them (`managed/Voxear.Scripts.props`), projects accept the
  launcher's version pin, `voxear-editor --project` opens one, `vox new` makes one (Empty or 3D) for the launcher's
  form, and `voxear-dist` packages a checkout as a release archive with its `engine.toml`. Signed nightly releases
  are published with the manifest the launcher reads (CookieJarTools/VoxearReleases). Scripts' interop reads and
  writes C#'s values unaligned, which a Release `Voxear.Core` needs.
- **C# hot reload:** changed scripts build in the background and reload, into the running game while playing: every
  behaviour is made again from its new class under the same handle with its fields carried over (private ones,
  script types, references to behaviours), OnDisable and OnEnable but no Awake or Start, and the old build unloads,
  checked to have left memory. `vox run`'s window reloads them too. A build that hangs is stopped.
- **Windows, drawers and workspaces:** a context menu on panel tabs (Close, Close Other Tabs, Reset Layout), a
  Window button listing every panel, the rail opening Levels and Settings as a drawer, the World and Profiling
  workspaces each keeping its layout, and dropdowns as wide as their options and kept on screen.
- **The Scene view's header:** Lit, Unlit and Wireframe shading, gizmos for lights and cameras with the selected
  one's reach or frustum, and full screen.
- **The Project's files:** context menus to open, show, copy the path, rename (with the `.meta`, and a script's
  class), duplicate, delete into `.voxear/Trash`, and make folders, scenes, levels, materials and scripts; and the
  Inspector for what is selected there, editing materials in place.
- **Prefabs in the editor:** made by dragging an Obj onto the Project, opened on their own in prefab mode with a
  way back, saved, and reloaded by their instances; components locked for designers with the values they still
  see, and scripts moved up and down.
- **The Profiler:** the last 300 frames' CPU and GPU times as a chart, and one frame broken down as Unity's
  Hierarchy does: samples with their total and self time and calls, from the editor's stages and panels, the
  engine's steps and systems, each behaviour's callbacks by class and method, and scripts' own
  (`Profiler.Sample`, `ProfilerMarker`); the GPU's passes, and what was drawn, culled and loaded.
- **The object picker:** reference fields, the Mesh Renderer's mesh and material among them, open a search of what
  they can take, with icons and where each is, narrowed as it is typed and picked from the keyboard.
- **Previews:** materials, prefabs, models and meshes drawn on their own into a shared atlas, a few a frame, and
  shown in the Project panel, reference fields, the object picker and a large Inspector preview a drag turns. Any
  size, background, rotation, offset, zoom, field of view and light, from Rust (`Renderer::add_preview`,
  `AssetPreviews`) and C# (`Preview.Of`), read back and saved as PNG. Voxear UI gained images.
- **Build profiles and the Build window:** `.buildprofile` files that change a base profile key by key, with a
  Release and a Development profile per platform in new projects; levels, development options, C# defines, player
  settings and the version. `vox build --profile` builds the scripts, the content the profile's levels need, and a
  player (`voxear-player` under the game's name) with what it runs in `Data/`, and writes `build-report.json`. With
  `--progress json` it reports in JSON lines and stops on `cancel`, which the editor's Build window follows in its
  own process: the profiles and their settings, each stage with its time, the content's progress, the log, Cancel,
  Run and Show in Files.
- **The layout kept:** the editor opens with the panels and workspace it was closed with, each workspace's layout
  kept in `.voxear/Editor.toml`.
- **VoxDocs editing:** a page's title, tags and Markdown with the page as it will read beside it, saved by Save,
  Ctrl+S, Done or opening another page, and renamed by its title. Multiline text fields scroll to the caret and
  with the wheel, and every text field undoes and redoes.
- **Pictures in VoxDocs:** PNG and JPEG pictures beside a page or in the project, shown at their size or fitted to
  the page; the project's `Docs` are no longer scanned as assets. Voxear UI gained `aspect-ratio` and mip-mapped
  images.
- **VoxDocs tags and headings:** tags over the page tree filter the pages, and "On this page" lists a long page's
  headings beside it, following the reading.
- **More of Sifter in the Console:** category colours kept with the project, saved filters, hiding chosen
  assemblies in traces, and the player's log from Run in the Build window.
- **VoxDocs Docs Health and home:** pages out of date, links that go nowhere, missing pictures, and pictures no page
  shows, each a link to its page or file; and home lists the pages updated lately and how healthy the docs are.
- **Live assets in VoxDocs:** a page's picture naming a prefab, model or material shows its preview, kept up to date.
- **`vox upgrade`** for the launcher: a project moved onto this version, pinned and given what this version's
  projects have, each change said in plain words, refused before any change for files from a newer Voxear.
- **The player's log kept:** a built game writes its log to the user's data folder, the run before's beside it, and
  the Console's Player Log reads either in.
- **Bundle compression:** LZ4, quick to load, Zstd, smaller, or none, from the project's `[content]` or a build
  profile, chosen in the Build window too, and recorded in the build report. A build log line naming a file shows it
  in the Project panel when picked.
- **Editing build profiles:** the Build window edits the picked profile in place, writing only what differs from
  its base; its log filters by level, and a build finishing behind other windows says so on the desktop.
- **The Voxear Manual in VoxDocs:** the engine's own pages, in `manual/`, shown after the project's in every
  project, shipped in each install and read but not written there. First pages: getting started (installing, a
  first project, the editor, playing, building), Objs and levels, scripting in C#, and VoxDocs itself; a test keeps
  every link and picture in it working. `vox doctor` no longer asks an install for the shader compiler.
- **The manual's models, lighting and debugging pages:** models and materials, lights, the sky, cameras and
  post-processing, and finding problems with the Console, the Profiler, the Frame Debugger, the Entities panel and
  a running game's own tools. The command bar now tries a query's first letter wherever it is, so "tool" finds
  "Rotate tool" before titles whose letters are only scattered.
- **The manual's game UI, ECS and content pages:** a first menu, UI markup, style sheets and UI from scripts;
  components, systems and queries from C#; and what goes into a build, bundles, and loading and memory.
- **VoxDocs' Code Reference:** a page for every public type of the game's scripts and of Voxear.Core, made from its
  `///` comments each time the scripts build, and linked to from any page as `[[code:Voxear.Prefab]]`.
- **Guide checks written in C#:** `[DocsCheck]` static methods, listed by ScriptInfo and in Action…, that a page's
  `check:code/` step runs through the scripts against the edited level, starting them without playing. That finishes
  the VoxDocs parts the Cookbook has.
- **VoxDocs' tags while writing and among many:** a page's tags as chips with the project's tags suggested, and a
  box that finds a tag when there are more than show at first.
- **Scene captures and GIF recordings in VoxDocs:** Capture Scene, Record Scene and Record Game put a PNG or a
  three-second GIF in the page being written, from the renderer's new captures; GIFs play on a page.
- **Image…, Link… and Action… while writing a VoxDocs page**, and a guide's step that makes an asset where it says.
- **Pictures dropped on a VoxDocs page** while writing it: copied beside the page and put in at the cursor.
- **A live material's fields on a VoxDocs page**, under its preview, changed and saved there as in the Inspector.
- **Docs Health by git:** a page is out of date by git's last change of it and of the files it links to, not the
  date in its header, and Health counts the comments the project's types still need.
- **Notes, code checks and Copy [Documentation] in VoxDocs:** Edit on a Code Reference page writes its note,
  kept in `Docs/.code-notes`; Docs Health finds notes for code that is gone and lists the project's types with no
  page; and a page's menu in the tree copies its link or a `[Documentation]` attribute.
- **Windows archives and the release workflow:** `voxear-dist` packages for Windows when run there, and a
  workflow builds both platforms under one version and, when asked and given the signing secrets, signs and
  publishes them for the launcher. Neither has run yet: that needs GitHub's runners.
- **The Console's settings screen**, as Sifter has it: a live sample drawn by the Console's own code over row
  height, colour by kind, alternating rows, how a category shows, the stack's parts, colours, and how many lines are
  held. That finishes what the Console takes from Sifter.
- **Step-by-step guides in VoxDocs:** steps that run a command by its title and checks that tick themselves for an
  Obj, a component or a file, with how far each guide has got kept for each person.
- **The Code Reference by folder:** inside each namespace, a type's page sits in the folders its source is in, read
  from the debug symbols, so the engine's 90-odd types are found by area rather than in one long list.
- **`[Documentation]` and `[Excerpt]`:** a book button in the Inspector that opens a behaviour's or field's VoxDocs
  page, a page's section shown above a field and kept up to date with the page, and each page's In the code. Voxear
  UI now breaks a padded label's words to its content width, so wrapped text in a narrow table cell no longer loses
  its last line.
- **Themes in OKLCH:** every colour of the default theme made from a base, an accent and a contrast, as Linear makes
  its themes, so dark, light and custom themes hold together. Dark gives the default theme's own colours; Light and
  Custom are picked in the Settings drawer's Appearance group, a colour from its swatch.

Next in M3:
- UI core:
  - [x] keyframe animations
  - [x] floating windows: Float on a tab's menu, or a tab dropped outside every area; moved by the bar, resized by
    the corner, docked again by a tab, and kept with the layout
  - [x] backdrop blur behind the viewport overlays (`backdrop-filter: blur()`)
  - [x] the C# UI API: `UiDocument.Load` and `Find`, text, classes, visibility, values and styles, and `Clicked`;
    the game's UI drawn over the Game view while playing, and over a player's window
  - [x] pack `.vui` and `.vss` files into built games: every document is a root, and its sheets ship with it
  - [x] UI built in code, as Unity's UI Toolkit: `UiDocument.Create()`, `new Panel()`, `Label`, `Button`,
    `Toggle`, `Slider`, `TextField`, `ScrollView`, `Foldout`, `Dropdown` and `Icon`, `Add`, `Remove` and `Clear`,
    and `Changed` with each kind's own event
  - [x] list and tree views at runtime from C#, filled from code: `ListView<T>` and `TreeView<T>`, with `MakeRow`
    and `BindRow`, rows filled in the frame they show in, and the selection
  - [ ] **Project Settings**: a pass that exposes as much of the engine as Unity's Project Settings do, well
    organised, so a game can be tuned for each platform and quality level without touching code:
    - a Project Settings window with a category list down its side, a search across every setting, values changed
      from the default marked, reset per value or per category, a tooltip and a manual link on each, and changes
      shown live in the editor
    - one `.toml` file per category in `ProjectSettings/`, written only where it differs from the default, so they
      read well in version control; build profiles change them key by key, as they do now
    - per-platform overrides (Linux and Windows now, more as platforms come), as Unity's platform tabs, and
      per-quality-level values for anything a quality level should change
    - **Quality**: named levels (Low, Medium, High, Ultra, and the project's own), a default per platform, chosen
      by players at runtime; each level sets render scale and upscaler, shadows (resolution, distance,
      cascades, softness), the LOD bias and lowest LOD shown, texture mip limit and anisotropic filtering,
      anti-aliasing, ambient occlusion, GI, reflections and volumetrics, particle and decal budgets, and vsync
      and the frame cap
    - **Player**: company and product name, version, icon and splash, the window (resolution, full screen or
      windowed, resizable, running in the background), the cursor and the display scale
    - **Graphics**: the default lighting look, HDR output, the default tonemapper, mesh shaders or their fallback,
      GPU decompression, and shader variants kept or stripped
    - **Physics and Physics 2D**: gravity, the fixed step, solver iterations, sleep, default friction and
      bounciness, body limits, and the layers and collision matrix that exist already
    - **Time**: the fixed step, the largest frame step, and the time scale
    - **Audio**: sample rate, buffer size, the most voices at once, default bus volumes, Doppler and the speed of
      sound, and Steam Audio's quality (HRTF, reflection rays, baking)
    - **Input**: the default actions, stick dead zones, default sensitivities, and input while unfocused
    - **Scripting and compilation**: the C# language version, nullable, warnings as errors, defines per platform,
      optimisation, trimming, and the script execution order
    - **Content and loading**: compression, streaming and memory budgets, and default import settings per asset
      kind
    - **Localization**, **Saving**, **UI** (the default theme, font and scaling) and **Tags and Layers**, brought
      in from where they are kept now
    - C#: every setting read as a typed value (`Physics.Gravity`, `Time.FixedDelta`, `Quality.Current`), the ones
      that may change while the game runs set from code, and quality levels picked by reference, never by name
    - a game's own settings: a class marked `[ProjectSettings]` gets its own category in the window, saved with
      the project and readable from code, as Unity's SettingsProvider
    - **Build settings**, in build profiles and edited in the Build window, so each platform and release ships as
      it needs, on top of the levels, defines, version and compression profiles have now:
      - the target: platform and CPU architecture, the output folder and the names of the executable and its
        data, and a clean build or one that reuses what has not changed
      - scripts: CoreCLR or Native AOT (M12), trimming, optimisation, debug symbols kept beside the build or left
        out, and C# defines per platform and per profile
      - development builds: the Console and dev tools in the game, the profiler able to connect, script debugging
        and waiting for a debugger at start, deep profiling, and extra checks, each its own switch
      - content per platform: texture compression and the largest texture size, audio compression and sample rate,
        mesh compression, the quality levels shipped and the shader variants they need, and what is stripped
      - identity: the icon at each size, the splash screen and its logos, the company and product names, and the
        version with its build number counted up by itself or taken from the git commit
      - packaging: a plain folder, a zip, an AppImage or Flatpak on Linux and an installer on Windows, code signing
        where the platform has it, and Steam depots ready to upload
      - the command-line arguments a build accepts, and the ones it starts with by default
      - every setting the same from `vox build` as from the Build window, so builds on a server match the editor's
  - [ ] **Incremental builds, kept builds, and running them from the editor:**
    - incremental builds: scripts, content bundles and the player each rebuilt only when what they are made from
      changed, so a build after a small change takes seconds; a clean build still on offer, and the report saying
      what was reused
    - builds kept, as an option per profile: each new build goes into its own versioned folder (the version and
      build number, the profile and the date) rather than over the last, with how many to keep before the oldest
      go
    - a Builds list in the Build window: every kept build with its version, profile, date, size and whether it
      succeeded, to run, show in files, compare with another, or delete
    - running any kept build from the editor, with its log in the Console, and arguments to pass it
  - [ ] **Debug tools that connect to builds**, so the Profiler and the other panels look into a running build as
    they do into Play mode:
    - **Connect**: the editor finds development builds running on this machine (and on the local network, later)
      and lists them with their name, version and platform; picking one connects the Profiler, Console, Frame
      Debugger, Entities panel and Audio panel to it
    - **Launch and Connect**: runs the picked profile's latest build, or a kept one, and connects as it starts, so
      the first frames are caught; with "wait for the editor" the build holds at startup until it is connected
    - a connection chip in each panel's bar saying what it is looking at (the editor, Play mode or a build), to
      switch between them, and a build that closes or drops leaving its last capture readable
  - [ ] **The Inspector's Debug mode**, as Unity's, switched from the Inspector's menu, each Inspector its own:
    - every value of a script, private and `[NonSerialized]` ones too, read-only where the Inspector would not
      write them, with the attributes that group, hide or draw them set aside, so what is really there shows
    - every component on the Obj, the engine's own as well (its entity, Name, Parent, World Transform, prefab
      links and behaviour handles), and a C# component's raw fields
    - references shown as what they hold (an asset's GUID and path, an Obj's entity and scene path) beside the
      usual picker
    - while playing, the live state behind the components: a body's velocity and whether it sleeps, a voice's
      position and volume, the LOD shown, and an animation's time once there are animations
    - for an asset, its GUID, its `.meta` and import data, and what it depends on and what uses it
    - copy a value, or its path in the scene file, from a row's menu
  - [ ] **Finding a setting by what it does**, for when its name is not known: one search over every setting the
    editor has (Project Settings, the Lighting panel, the Settings drawer, Preferences, the Inspector's components
    and import settings) that matches rough words against each setting's name, group, tooltip and description,
    and words that mean the same (shimmer, sparkle, flicker), not only the name:
    - in the palette (Ctrl+K) and in each settings window's own search, ranked by how well each matched, with the
      words it matched marked and the line of the tooltip that matched shown under the name
    - picking one opens its page or panel, scrolls to it and flashes its row, and selects what it belongs to
      where it is on an Obj or asset
    - settings hidden for now, as those of a feature turned off, still found, saying what turns them on
  - [ ] **UI navigation**, for gamepads and keyboards, as Unity's and Unreal's, so a complicated menu works with a
    controller without the game writing any code for it:
    - automatic: up, down, left and right move to the nearest focusable element that way, by distance and by how
      well it lines up, across nested panels, into and out of scroll views (scrolled to as it is reached), lists,
      grids and tabs, and past hidden and disabled elements
    - per-element modes, as Unity's Selectable: automatic, horizontal or vertical only, none, or explicit, with
      wrapping at a container's edges, and focus kept inside a popup or dialog while it is open
    - explicit links in markup and from C# by element reference, never by name: `button.Navigation.Down = quit`
    - Submit and Cancel from the input actions; sliders, dropdowns and toggles changed with the stick or arrows
    - the first focused element per document and per screen, each container remembering where its focus was, and
      the focus ring shown for the controller and the keyboard, hidden once the mouse moves
    - events for focus in and out, and a navigation move a script can redirect or cancel
    - an editor overlay that draws each element's navigation arrows, as Unity's Visualize button does
- **Inspector with Odin-style attributes** (see "The Inspector and its attributes" in the architecture):
  grouping, conditions, value controls, validation, buttons and table lists, from source-generated metadata, plus
  custom drawers in C#.
  - [x] the first part: groups (foldout, box, horizontal, tabs), titles, order, indent and labels; show, hide,
    enable and disable on another field's value; read-only and hidden; range sliders, min and max, suffixes,
    multiline text, progress bars, enum buttons and larger previews; info boxes, required references and
    tooltips; described by ScriptInfo
  - [x] buttons, value-changed callbacks, input validation and value dropdowns, which run the behaviour's code in
    edit mode, on a copy of it made from the scene's values
  - [x] list and table drawers, and colour palettes: `[TableList]`, `[ListDrawerSettings]` with pages and index
    labels, and `[ColorPalette]`
  - [x] the build's check of every scene and prefab: `[Required]` and `[ValidateInput]` problems listed as the
    build's warnings, and kept in its report
  - [x] one collection field for every list, as Unity 6's: each item on a ground of its own, dragged by its grip
    into a new place while the others slide out of its way and ease into place; scripts' lists and arrays, a
    level's scenes and the Build window's lists all use it
  - [x] the analyzer: a wrong name or shape in an Inspector attribute is a compile error where it is written
    (`VOX0001` to `VOX0007`); the source generator is not needed, as ScriptInfo describes the scripts already
  - [x] custom drawers in C#: `ValueDrawer<T>` and `AttributeDrawer<TAttribute, T>`, built from `.vui` markup and
    drawn through the C# UI API, chained through a `next` element around the Inspector's own control
  - [x] drawers built in code, in `Build(root)`, with markup optional
  - [x] custom inspectors that draw a whole behaviour, as Unity's `Editor`: `BehaviourInspector<T>`, with the
    behaviour's usual fields in its `next` element
  - [x] drawers and inspectors written only in `Build`: handlers given as elements are made (`new Button("Fill",
    Fill)`, `.OnClicked`, `.OnToggled` and the rest) that set `Value` or change `Target`, saved as one step to
    undo; `Refresh` and `Bind` to keep elements in step; `NextField` for markup's `next`; `Draw` optional
- A build option to compile the scripts ahead of time (Native AOT) as well as run them on CoreCLR, which needs the
  player and Voxear.Core built that way too, and an option to ship the .NET runtime with the game (M12).
- A jobs API for fast parallel C#, Voxear's answer to Unity's job system without Burst: work split over the
  engine's worker threads on `Span`s of plain structs, with dependencies between jobs, `System.Runtime.Intrinsics`
  for SIMD, and an easy `Jobs.For(count, i => ...)` beside the advanced form. Heavy engine work stays in Rust.
- For the launcher: the first-person, third-person, top-down and 2D templates (after an input API and 2D
  rendering).
- **More of the Voxear Manual**: each part as it is built (input, audio, physics, animation and the rest),
  written the same way: clear and short, plain words, each term explained where it first appears, an example
  before the details, and nothing that sounds generated.
- Debug tools:
  - [x] the stats overlay's frame-time graph; the Profiler's timeline view and managed allocations (GC Alloc)
  - [x] worker threads' samples in the Profiler, a lane each in the Timeline, and Tracy behind it
    (`--features tracy`: every sample a zone, each thread named, a mark at each frame)
  - [x] the frame debugger: each pass of the frame with its GPU time and images, and an image as it stood after
    its pass, at any mip, in one channel or all, with an exposure
  - [x] the ECS inspector: archetypes, entities and their components, the system schedule with timings, and a filter
    by components
  - [x] the ECS inspector's editing of plain entities' values, and chunk occupancy
  - [x] the memory profiler with snapshots and diffs: the process, Rust's heap, ECS chunks, each GPU allocation and
    the managed heap, in the Memory panel
  - [x] debug draw: lines, arrows, boxes, spheres and text, from C# and Rust, hidden behind the scene or on top
  - [x] the dev console: commands with arguments and console variables, in the game window's development tools
  - [x] the dev console through `vox`, into a running game: `vox console "line"`
  - [x] debug draw's texts in the player, as in the editor, from one shared piece
  - [x] a C# `Color` field in the Inspector: a swatch for the colour picker, set from hex or numbers in scenes
- **Saving**, built into the engine (see "Saves" in the architecture):
  - [x] `Save.Write("slot1", data)` and `Save.Read<T>("slot1")` with nothing to set up, `Save.Slots` with each
    save's time, version and summary, and slots deleted, copied and renamed
  - [x] any value: numbers, text, enums, the math types, dates, arrays, lists, sets and dictionaries, and classes,
    structs and records by their fields and properties, with shared and circular references and derived types
  - [x] Objs by their scene and place, behaviours that values point to by their Obj, assets (prefabs, sounds, and
    every asset type to come, through `IAsset`) and `Ref<T>` by GUID, and a behaviour's own fields, read back into
    it with `Save.ReadInto`
  - [x] `SaveConverter<T>` for a type saved its own way, found in the scripts by itself
  - [x] binary and JSON behind one `SaveFormat`, JSON in the editor and binary in builds, and formats of a game's
    own, found by themselves
  - [x] versions with `[SaveVersion]` and `[MigrateFrom]`, and saves from a newer version refused
  - [x] slots in the game's own save folder, each written whole and renamed into place, and checked when read
  - [x] `WriteAsync`, writing on a thread of its own while the game carries on
  - [x] `SaveFile`, for several parts in one save
  - [x] saves of the scene: `[Saved]` on a behaviour's fields, properties or whole class, Objs scenes loaded that
    were destroyed, and Objs scripts made from prefabs, made again with their parents, places and values, in
    `Save.WriteScene` and `await Save.LoadScene`, or `SceneState` as a part of a `SaveFile`
  - [x] `SavedTransform` for where a scene's Obj stands, and `Save.IsLoadingScene` for spawners
  - [x] C# components (`IComponent`) marked `[Saved]`, kept whole in a save of the scene for each Obj it can find
    again
  - [ ] a picture of the game with each save, for load menus (the file already has room for one)
  - [ ] a Saves window in the editor, to look into, change and delete the project's saves
  - Later: a store for cloud saves, with Steam's in M11.
- **C# ECS API**, the advanced path beside Behaviours (see "ECS and the Obj model" in the architecture):
  - [x] C# components as `unmanaged` structs, registered by name and layout, living in the same chunk memory as Rust
    components
  - [x] `SystemBase` systems placed in schedule stages, with queries over chunks that hand out `Span<T>` reads and
    writes with no copies, plus `With`, `Without` and `Changed` filters, and `ForEach` for the short way
  - [x] entity commands from C#: create, destroy, and add or remove components, applied at sync points
  - [x] a bridge to Objs (`obj.Add<Spin>()`, `obj.Get<Spin>()`), so Behaviours and systems can mix
  - [x] components written in scenes and prefabs, and shown in the inspector
  - [x] live component values in the Inspector while playing
  - Later: parallel jobs over chunks.
- The Animation layout, once there is animation.
- **From the first full try of the editor** (2026-11-01), the editor's own part:
  - [x] clicks and double clicks that land even when what was pressed is built again before the release
  - [x] text cut off at the edge of what hides its overflow, and resizable columns: the Entities panel's lists,
    and the Memory panel's lists and amounts table
  - [x] the Profiler: dragging along the chart goes through the frames, and the Timeline scrolls
  - [x] reference fields as Unity's: a click pings, a double click selects, and the arrow opens the picker
  - [x] Close Prefab on prefab mode's bar
  - [x] a Layout menu in place of the World and Profiling tabs, with layouts saved under names
  - [x] windows: an options button on every window with its menu (Maximize, Close, Float, panels to open, Reset
    Layout), the same on a tab's or its bar's context click, any panel can close, and a tab dropped in the middle
    of a panel floats rather than joining it
  - [x] script fields edited while playing, on the running behaviour, with its buttons and callbacks
  - [x] references rather than strings for levels and scenes: `LevelAsset` and `SceneAsset` fields, picked and
    dropped as prefabs are, and taken by `Level.LoadAsync` and `Scene.LoadAsync`; Hello's level cycle uses them
  - [x] a component field that takes an Obj with the component: an `AudioSource` field offers the Objs with an
    Audio Source
  - [x] several Inspectors, each with a padlock, opened from a window's menu (Add Inspector)
  - [x] the Scene and Game views drawn at once, each through its own camera, a floating one over the docked one
  - [x] Lit Wireframe shading in the Scene view: the lit scene with its meshes' edges over it
  - [x] several Scene views, each with its own camera, shading and Gizmos, opened from a window's menu (Add Scene
    View)
  - [x] handles for two axes at once: move's and scale's squares between each two axes, rotate's ring around the
    view and turning freely inside the rings
  - [ ] **A gizmo API for game code** (agreed, being built). Unity splits this into Gizmos, Handles and Debug, each
    with its own shapes, and gizmos are editor-only and one pixel thin. Voxear has one set:
    - [x] one drawing API callable from anywhere (Update, systems, worker threads), `Draw`, beside today's
      `Debug.Line`, `Box`, `Sphere`, `Arrow` and `Text`: polyline, ray, cross, circle, arc, capsule, cylinder,
      cone, frustum, bezier, grid and labels; lines of any width in pixels, dimmed rather than hidden behind
      geometry, or on top when asked; durations; all lines in one draw
    - [x] `Draw.ScreenText`: lines of text down the view's top-right corner, over the Scene and Game views and a
      player's window
    - [x] `Draw.WidthInMetres`: lines as wide as something that size in the world, narrowing with distance
    - [x] solid shapes: `Draw.SolidBox`, `SolidSphere` and `Triangle`, their sides shaded apart, only their near
      side drawn, fainter than lines behind the scene
    - [ ] still to come for drawing: wire meshes
    - [x] nestable scopes instead of global state: `using (Draw.Color(Color.Red))`, `Draw.InLocalSpace(obj)`,
      `Draw.Width(3)`, `Draw.Duration(2)`, `Draw.OnTop()` and `Draw.InGame()`
    - [x] `DrawGizmos(Gizmo gizmo)` on a behaviour, one callback in place of Unity's two, with `gizmo.IsSelected`
      and `IsHovered`, while editing and while playing, in the Scene views only
    - [x] gizmos that are clickable and select their Obj; a type's gizmos hidden from the Gizmos menu; and
      `[GizmoIcon(IconName.Audio)]` giving it an icon by typed reference
    - [x] attribute handles that edit fields in the Scene view with no code, undoable: `[PositionHandle]` (local or
      world, every item of a list joined as a path), `[RadiusHandle]`, `[BoundsHandle]`, `[ArcHandle]`,
      `[DirectionHandle]` and `[SliderHandle]`
    - [ ] the same with several Objs selected, and the move, rotate and scale handles on points inside an Obj,
      such as a path's points, picked by clicking them
    - [x] for custom tools, handles that return the new value, with undo and `[OnValueChanged]` built in:
      `Target = Handles.Position(Target)` (dragged across the view, so it is also Unity's `FreeMove`),
      `Handles.Radius`, `Handles.Slider` and `Handles.Button`, in `DrawGizmos`, while editing and while playing
    - [x] `Handles.Disc` for turning, a dot dragged round a ring (15° steps with Ctrl), and
      `Handles.Position(p, arrows: true)` with a move arrow along each axis
    - [x] in development builds, drawing shows in the game; release builds leave it out unless asked to keep it
    - [ ] later: physics queries drawn with their hits, and a recorder to scrub back through what was drawn
  - [x] several Hierarchies and Project panels, opened from a window's menu (Add Hierarchy, Add Project), and a click
    on the space below the Hierarchy's rows selects nothing
  - [x] windows a game's code can open more than once: `EditorWindow`, built with Voxear UI, opened from any
    window's menu as often as wanted, each its own instance, updated while editing and while playing, and kept
    with the layout
  - [x] windows dragged out of the editor into windows of the desktop's own: Float in Its Own Window on a window's
    menu, or a tab or floating window let go past the main window's edge; its UI and a Scene or Game view's 3D
    drawn there, input routed to it, closed from its own frame, docked again by dragging its tab onto the main
    window (through the desktop's drag and drop on Wayland), and kept with the layout

**Exit:** you can build and play the Hello scene in the editor, and it looks like the concept.

## Packages: sharing code, tools and content ✅ (done)

Voxear is meant to be open: a project's own code already adds editor windows, side bar buttons, commands, drawers
and Inspector attributes. Packages carry that from one project to another, as Unity's packages do, so people can
share tools, systems, shaders, art and samples and add them to a project in a click.

Done:
- **What a package is:** a folder with a `package.toml` (name, version, description, authors, the engine versions
  it works with, and the packages it needs). It holds C# in `Scripts/`, VoxDocs pages in `Docs/`, samples in
  `Samples/<name>/`, and assets everywhere else. `vox package new` starts one.
- **Where packages come from**, written in the project's `packages.toml`, whose comments survive Voxear's edits:
  - a Git repository by URL and a tag, branch or commit, with `path` for a package inside a larger repository.
    Git does the fetching, so a developer's own credentials and SSH keys work.
  - a folder on disk, for packages being written alongside a game
  - a registry: a plain Git repository of `packages/<name>.toml` listings, for finding packages by name and
    searching. The default registry is [CookieJarTools/VoxearPackages](https://github.com/CookieJarTools/VoxearPackages).
    Projects can name more under `[registries]`, and `VOXEAR_REGISTRY` sets a machine's default.
- **Resolving and locking:**
  - Versions are semantic. Ranges are resolved together with every package's own needs, and the project wins
    when it asks for a package from somewhere else. Errors name who asked for what.
  - `packages.lock` keeps the exact commit of each package.
  - Downloads go into a cache shared by every project on the machine. With the lock and the cache in step,
    nothing needs the network.
- **Scripts:** each package's C# builds into an assembly of its own, from a project Voxear writes in
  `.voxear/Packages`, against the packages it needs. The game's scripts use them all. The package assemblies load,
  hot-reload and ship with the game's, and their behaviours, windows, side bar items, drawers and commands are
  found as the game's are.
- **Assets:** a package's assets are mounted at `Packages/<name>/`, with IDs that do not depend on where the
  cache is. Import settings for cached packages are kept in the project's `Settings/Packages`. A build ships only
  the package assets the game uses. The Project panel has a Packages section, read-only for cached packages.
- **Docs and samples:** a package's pages show beside the manual in VoxDocs, and its samples copy into
  `Samples/<Package>/<Sample>`.
- **The Package Manager window** (Window > Package Manager):
  - what the project has, with updates
  - add by Git URL, by folder, or from a registry search
  - update, remove, read a package's README and pages, and import its samples
  - before code from somewhere new is added, a dialog shows what each package holds and asks
- **From the command line:** `vox package add`, `remove`, `update`, `list`, `search`, `new`, and `publish`, which
  lists a tagged release in a clone of a registry.

Next:
- Changes made outside the editor to a folder package's files, picked up and imported again as the project's are.
- Editing a package's string tables and input actions from the editor writes into a folder package, rather than
  failing.
- Cancelling a fetch in the Package Manager, and its update check working offline without an error.

Later:
- Native plugins (Rust or C libraries a package loads into the engine, behind the same consent).
- A place to browse packages with pictures and ratings, as Unity's Asset Store.

## M4 — Input, localization, audio and physics (in progress)

- Input:
  - [x] actions in maps, from the project's `.inputactions` file or Voxear's own: buttons, values and vectors;
    composites such as WASD; deadzone, invert and scale processors; hold and tap interactions
  - [x] the keyboard, the mouse and gamepads (gilrs), read raw too, through `Keyboard`, `Mouse` and `Gamepad`
  - [x] control schemes switching to the device used last, and `Started`, `Performed` and `Canceled` events
  - [x] rebinding: to a control named, or the next one pressed; conflicts found; bindings saved and read back
  - [x] prompts as text per scheme and pad family, with keys as this keyboard's layout types them
  - [x] the Game view hears the keyboard and mouse while it has the focus
  - [x] key maps: the keyboard and mouse, and a gamepad, drawn with each bound control lit and its actions labelled
    round the device, coloured as assigned, hidden and unused or by the actions' categories (`[[category]]` in
    `.inputactions`, and `hidden` actions), in the editor's Input Actions window and in games as `KeymapView`
  - [x] a ready-made, themeable rebinding screen (`KeyBindingScreen`) with a Layout view, the device drawn, and a List
    view (`ControlsList`), a row an action with its keys and buttons, conflicts and Reset, as tabs or alone; built
    from `Input.Actions`, `InputAction.GetBindings`, `Input.ControlUses`, `ControlLabel`, `Conflicts`, `Categories`
    and `Rebind`, which games' own screens use too
  - [x] editing actions and bindings in the Input Actions window, with listening for a binding: an Actions view of
    maps, actions with their bindings, and properties, saved to the `.inputactions` file as it changes
  - [x] prompt glyphs per device family: keycaps, the mouse's buttons, and each pad's face buttons as Xbox,
    PlayStation and Switch pads mark and colour them, in `<Prompt>` and `PromptView`
  - [x] local multiplayer pairing: `Input.Joining`, a player for each device a button is pressed on, each reading the
    actions from their own device, `Input.Players` and `PlayerJoined`
  - [x] maps that exclude each other (`excludes`), never on together and so not conflicting; conflicts marked in key
    maps and the Actions list; a device's outline behind its keys; icons for actions; controls picked from a
    searchable list by device, as Unity's binding picker, and typed in C# as `InputControl`, `BindingPart` and
    `InputMap` rather than path strings
- Localization:
  - [x] string tables (`.strings` files) holding each key's text in every language `voxear.toml` lists, read when the
    game starts and again when one changes, and shipped with every build; in C#, `Localization.Get`, `Format` with
    `{name}` values, `Language`, `Languages` and `Changed`
  - [x] fallback chains as ICU makes them (`pt-BR`, then `pt`, then the project's own); the game starts in the system's
    language on Linux; missing keys marked `⟦key⟧` while the game is made
  - [x] asset tables: `.assettable` files naming a key's asset in each language, falling back as text does, shipped
    with every build, and `Localization.Asset`, `Sound` and `Prefab` in C#
  - [x] ICU smart strings: plurals by each language's rules (ICU4X), exact numbers, select (as for gender), and numbers
    written as the language writes them
  - [x] dates and currencies in smart strings (`{when, date}`, `{when, time}`, `{price, currency}`), with `Money` and
    `Currency` in C#
  - [x] binding through `@key` in UI markup and in an element's `Text` from a script, shown again when the language or
    the text changes
  - [x] `LocalizedString` fields, picked in the Inspector from the project's keys beside their text, saved as the key
  - [x] observable text, `Localized`, following the language, its table and its values, with `Changed`; from a
    `LocalizedString` by `Observe()`
  - [x] keys' text on elements made in code: a `LocalizedString` for a label's, button's, toggle's or foldout's
    text, `Localize(key, values)`, tooltips, placeholders and dropdown options from keys, and typed keys made
    from each table by a source generator (`Strings.Menu.Play`)
  - [ ] a `LocalizedText` component, once there is text in the world
  - [x] CSV export, and import from a file with what it adds, changes and takes away shown first (`vox loc export` and
    `vox loc import`)
  - [x] CSV export and import from the editor: Export CSV and Import CSV under a string table in the Inspector, with
    what it adds, changes and takes away shown before Apply
  - [x] CSV import from a URL, a Google Sheet's link read as its CSV and kept with the table (`sheet`), in the
    Inspector and `vox loc import`
  - [x] pseudo-localisation from the palette, accented, a third longer and in brackets, while editing and playing
  - [x] string tables edited in the Inspector: each entry's key, its text in each language and its comment, missing
    text and text past its limit marked, entries dragged into a new order
  - [x] a Localization Tables window: every table's keys against the languages, with search, find usages, and
    renaming a key everywhere it is used
  - [x] the saved choice and Windows' languages in language detection: the language a script picks kept beside the
    saves and started in next time, and Windows' preferred UI languages
  - [x] the store's language (Steam's) in language detection, through the developer's own Steamworks library beside
    the game, loaded when it is there; Voxear ships none
  - [x] RTL mirroring: a game in a language written right to left lays its UI out that way, rows from the right,
    sides swapped and text from the right; `direction: ltr` keeps a part as it is, as key maps do
  - [ ] per-language fonts, once fonts are assets of a project's own
- Audio:
  - [x] a Rust mixer on cpal: music and effects buses under a master volume, and voices that play, loop, pause and
    stop, with long music streamed
  - [x] 2D and 3D sources, with distance falloff and panning from the listener (the camera, or an Audio Listener)
  - [x] sound assets (WAV, OGG Vorbis, FLAC and MP3) with import settings, previewed in the Inspector, and the Audio
    Source component; a sound dragged into the scene makes an Obj playing it, and background loads decode sounds
  - [x] sound moves to the new default output device when the old one goes away
  - [x] C#: `Audio.Play(sound)` and `Audio.Play(sound, at:)`, `Sound`, `AudioSource` and the bus volumes; builds pack
    the sounds they use
  - [x] Doppler on 3D sources, from their and the listener's speed, with a Doppler Level per Audio Source
  - [x] effects, snapshots and ducking on the buses: a low-pass and a high-pass on each, `AudioSnapshot` with
    `Audio.TransitionTo`, and the music ducked under loud effects
  - [x] Steam Audio 4.8.1, loaded when it is there: HRTF for headphones on 3D sources, `Audio.Headphones` and
    `[audio] headphones`, and player builds that take the library and its licence with them
  - [x] Steam Audio occlusion and transmission: raycast or volumetric per Audio Source, through the level's still
    colliders, each with an acoustic material, in the Inspector, scene files and C#
  - [x] reverb as the listener's room rings, from Steam Audio's reflections off the same walls, with `Audio.Reverb`
    and `[audio] reverb`
  - [x] Steam Audio pathing: Bake Sound Paths in the editor stands probes on the level's floors and bakes the paths
    between them into a `.paths` file beside the level, which builds pack with it; an Audio Source with Pathing on
    is heard round the walls along them, from the doorway's side, in the Inspector, scene files and C#
  - [x] Steam Audio convolution reflections: the room's own echoes, worked out from the listener and played through
    the HRTF with headphones on, with `Audio.ReverbMode` and `[audio] reverb_mode`; parametric stays the default and
    the fallback. Ray-traced audio follows in M5, once the ray-tracing acceleration structures exist.
  - [x] the audio debugger: each Audio Source a speaker in the Scene view, lit while it plays, and the selected one's
    min and max distances as rings
  - [x] a list of the voices playing, with their buses and volumes: the editor's Audio panel
- Physics:
  - [x] Jolt 5.6 vendored and built behind Voxear's own C shim: shapes, static, kinematic and dynamic bodies,
    raycasts and overlaps, contacts, joints and a character controller
  - [x] Collider (box, sphere, capsule, trigger), Rigidbody and Character Controller components, in scene files and
    prefabs, in the Inspector under Add Component's Physics folder, and outlined in the Scene view
  - [x] bodies kept in step with their Objs as the game plays, stepped after `FixedUpdate`; contacts and trigger
    crossings that last through sleep, and characters that triggers notice
  - [x] C#: `Collider`, `Rigidbody` (velocities, forces, torques, explosions, sleep) and `CharacterController`
    (`Move`, `SimpleMove`, grounded), `Physics.Raycast`, `Linecast`, `OverlapSphere` and gravity, and
    `OnCollisionEnter`/`Exit` and `OnTriggerEnter`/`Exit` straight after each step
  - [x] joints as components: fixed, hinge, ball and distance, held to another Rigidbody or the world, with limits,
    in scene files, the Inspector, the Scene view and C#
  - [x] layers: 32, named in the Inspector as Unity's Tags and Layers, with a Collision Matrix that keeps 3D and 2D
    bodies and characters apart; rays and overlaps take a layer mask; `LayerMask` and `IgnoreLayerCollision` in C#
  - [x] mesh and convex colliders from models: a Mesh collider takes its Obj's mesh's triangles, or their convex
    hull, read once from the model (or the built content) when first needed, outlined in the Scene view
  - [x] 2D physics on Box2D v3 (through `boxdd-sys`): Collider 2D (box, circle, capsule, trigger) and Rigidbody 2D
    in scenes, prefabs, the Inspector and the Scene view, and `Collider2D`, `Rigidbody2D`, `Physics2D` and the
    `On…2D` messages in C#
- **Built-in vehicle physics**, on Jolt's vehicle constraints:
  - [x] wheeled vehicles (cars, trucks, motorbikes) with suspension, engine and gearbox, and differentials: a
    `Vehicle` beside a Rigidbody, driven on the `Wheel`s of its child Objs, whose Objs physics turns and drops
  - [x] tracked vehicles (tanks), turning on the spot
  - [x] the easy path: a ready-to-drive Car from the Hierarchy's + and the palette, and `Throttle`, `Steering`,
    `Brake`, `Handbrake`, `Speed`, `Rpm` and `Gear` in C#; in the Inspector, scene files and the Scene view
  - [x] the advanced path, first part: a gearbox of so many gears from first's ratio to top's, anti-roll bars across
    each axle, and each tyre's grip forwards and sideways, in the Inspector, scene files and C#
  - [ ] tyre friction curves and the engine's torque curve, once Voxear has a curve field
  - input bindings for driving, and a debug view of wheel contacts, slip and suspension

## M5 — Lighting (in progress)

Done so far:
- **Per-meshlet culling**, with mesh shaders where the GPU has them and a compute path everywhere else. Both draw
  the same image as whole LODs, to the pixel.
  - Each LOD is cut into meshlets of 64 vertices and 124 triangles, with a bounding sphere and a normal cone. They
    are stored in the cooked format.
  - Meshlets are tested against the frustum in every list (each shadow cascade against its own). The main view
    also tests their normal cones, and the late list the depth pyramid.
  - Neighbouring surviving meshlets draw as one range. The work list is flattened, so no GPU threads sit idle on
    small LODs, and stats are counted in GPU memory.
  - Counters for meshlets tested, culled and drawn appear in `vox render` and the stats overlay. A Meshlets debug
    view (F6) colours them.
  - `--no-mesh-shaders` and `--no-meshlet-culling` are there to compare.
  - On an RTX 3070, mesh shaders take Stress at 1440p from 1.57 to 1.46 ms. With every sphere at full detail, 30% of
    the view's meshlets face away and are culled.
- Still to come for it: double-sided materials, so cone culling can skip them and shadows can cull back faces
  where the material allows.
- **Ambient occlusion (GTAO)**, as Jimenez et al. and Intel's XeGTAO work it out, from depth drawn before shading.
  - The two-phase culling's passes draw depth first, then shade each pixel once.
  - Ambient occlusion takes only the sky's light away, with specular occlusion for reflections; the sun and lamps
    are untouched. Specular occlusion darkens only the part of a reflection that comes from the probes: a traced
    or screen-space reflection already stops at what is in its way.
  - At half resolution by default: each frame searches one pixel of each 2x2, and the temporal pass, at full
    resolution, fills in the rest from what lies on the same surface.
  - Settings in `[post.ambient_occlusion]`: radius, strength, a tint for stylised looks, quality, and
    `full_resolution`.
  - An AO debug view (F7, or `--debug-view ao`).
  - On an RTX 3070 at 1440p it costs about 0.35 ms (Stress), and full resolution about three times that.
- **Specular anti-aliasing** (Kaplanyan and Tokuyoshi): a shiny surface is drawn rougher where its normal turns a
  lot from one pixel to the next, as over a normal map's fine bumps, so its highlights and reflections do not
  sparkle as the camera moves. `specular_smoothing` in `[post.anti_aliasing]`, 0.5 by default, is Shimmer
  reduction in the Lighting panel. The depth passes write the same roughness, so reflections fade there too.
- Still to come: the post-processing settings, AO's among them, in the editor and the C# API; today they are in a
  scene's `[post]` section.
- **Area and tube lights:** rectangle and disc lights shaded by linearly transformed cosines (Heitz et al. 2016,
  tables fitted to GGX ahead of time), and tube lights by Karis's representative point. In scenes, the
  Hierarchy's + menu, the Inspector and the Scene view, which shows each one's shape and reach.
- **IES profiles and light cookies:** `.ies` files (LM-63, 1986 to 2002) as assets a point or spot light takes,
  baked over the sphere; and pictures as cookies, over a spot light's cone or tiled over the world from the sun.
  Both are fields on the Light in the Inspector, and scenes save them.
- **The atmosphere:** `[sky] mode = "atmosphere"`, a physically based sky as Hillaire models it (2020), with
  transmittance, multiple scattering, the sky view and aerial perspective drawn as tables, so the sky reddens at
  sunset, lights the scene from its own colours and hazes what lies far off. About 0.06 ms a frame.
- **Shadows from point, spot, area and tube lights:** a light's `shadows` (Shadows in the Inspector, on by
  default; `shadows = false` turns them off) draws what is in its reach into views of a 4096×3072 atlas of its own, 512 texels each: a spot light one
  along its cone, any other six, a cube's faces, each a little wider so the filter stays inside it at the rim. The
  eight nearest the camera, 48 views between them, cast shadows in a frame; each has a list of its own that the
  culling fills by its reach and draws into each of its views, on every culling path. Without depth clamping, so
  a flame or bulb round a light, nearer than its near plane, is clipped rather than shutting it in. Shading, glass,
  the fog's light shafts, reflections and the GI's rays all read them, with nine taps of 2×2 PCF and a normal
  offset. In Sponza's demo, its five candles, the Cold Lamp and the Neon Tube cost about 0.16 ms on an RTX 3070.
  GPU test: a point, spot, rectangle and tube light each shade the floor behind a box with a flame round it, on
  every culling path.
- **The C# `Light` API:** `Obj.Light` and `Obj.AddLight(kind)`, with its kind, colour (sRGB, kept linear),
  temperature, intensity, range, spot angles, shadows, shape and softness.
- Still to come for lights: caching local shadows of what does not move, and soft shadows from area lights.
- **Volumetric fog:** `[post.fog] volumetric = true` lights the height fog near the camera as a 128×72×64 froxel
  volume: the sun through its shadow cascades, so shafts show between shadows, every clustered light, and the
  sky, with Henyey-Greenstein scattering. Each froxel is sampled at a jittered depth and blended with last frame's
  volume, then integrated from the camera out; beyond `volumetric_distance` the plain height fog carries on. About
  0.17 ms on an RTX 3070, whatever the screen's size. A GPU test checks the fog is dark in shadow and glows round
  a lamp in its colour.
- **Soft shadows and contact shadows:** the sun's shadows are percentage-closer soft shadows (PCSS), with a
  Shadow softness on the light, the sun's width in degrees (0.5 by default, 0 for hard edges): a blocker search,
  then a filter as wide as the blocker is far above, so shadows stay sharp where things touch. Screen-space contact
  shadows march a short way towards the sun from each pixel, for the small shadows the shadow map cannot hold,
  even beyond its 80 metres. On Stress at 1440p, PCSS adds about 0.11 ms and contact shadows 0.23 ms.
- **Colour grading lookup tables:** `[post.grading] lut` names a strip of N×N squares (1024×32 or 256×16, as
  Unreal lays them out), uploaded as it is and applied last, in sRGB, by `lut_contribution`. `vox lut neutral`
  writes the table to grade in an image editor.
- **Local post volumes:** a Post Volume component (global, box or sphere) with its own `[post]` overrides, a blend
  distance, a weight and a priority. Where the camera is, each volume's settings blend in, lowest priority first:
  numbers and colours smoothly, switches and choices past half. In scenes, the Inspector, the Hierarchy's +
  menu and the Scene view, which draws its shape and how far it blends.
- **Voxear's temporal upscaler:** `[post.anti_aliasing] render_scale`, as Unreal's screen percentage. The scene
  is drawn smaller, then TAA builds each output pixel from the drawn pixels round it, weighted by where their
  jittered samples fell, and from its clamped history; the jitter sequence lengthens as the scale drops, so the
  samples cover every output pixel. Depth of field and motion blur now run before TAA, as in Unreal, at the drawn
  size. On Hello at 1440p a frame takes 1.42 ms at 0.67 and 1.20 ms at 0.5, against 2.29 ms at full size.
  `vox render --render-scale` compares them.
- **Real-time voxel GI, first version:** `[post.global_illumination]`. A clipmap of three nested 64³ voxel grids
  round the camera, each with twice the last's voxel size, in two images used in turn. Each frame, after the
  opaque passes, compute passes shift last frame's voxels to the new origins and fade them, mark voxels where the
  sun's shadow cascades see surfaces, and write the lit colour of every other pixel into its voxel. Shading traces
  five 60° cones through last frame's voxels for bounced light and the sky's light where it is open, in place of
  the flat sky ambient. Light bounces again frame to frame. About 0.75 ms at 1440p on an RTX 3070; off by
  default. GPU test: a sunlit red wall reddens the floor before it, and the floor behind it is darker.
- **GI's radiance cache:** a 48³ grid of probes, two voxels apart, each holding the light reaching it as an
  average colour, how open its voxel is, and the direction the light grows brighter in (L1). A compute pass traces
  twelve cones from an eighth of the probes each frame, and from any the grid has just moved over, blending with
  their last value. Shading reads the eight probes round each pixel, weighted towards those it faces and away from
  those inside walls, falling back to the plain sky beyond the grid. GI went from about 0.95 ms to 0.28 ms at 1440p.
- **Reflections:** every surface reflects the sky (or the atmosphere) in its mirror direction, blurring into the
  ambient light as it roughens, rather than the ambient light alone. Screen-space reflections, at half
  resolution: each pixel's mirror ray marched through the depth buffer, crossings refined by halving, the colour
  taken from last frame's TAA picture where that point was. On in the Voxear Look; needs TAA. About 0.16 ms at
  1440p. GPU test: a polished floor mirrors the cube on it.
- **Stylised shading:** a sun's shadow tint, a colour added where the sun does not reach (in shadow and on sides
  turned from it), lit by the sun's own colour; and foliage materials, lit with wrap lighting, light through the
  leaf against the sun (its shadow read from the leaf's sunlit side), and a rim of the sun's colour. Bloom takes a
  tint. GPU tests: a tinted shadow, a leaf lit from behind, a gold glow.
- **Lighting looks, as presets:** `[post] look = "soft_overcast"` or `"storybook"` sets the sky, the sun's colour,
  strength, softness and shadow tint, and the post-processing together; the scene's own `[sky]`, its light's own
  settings and any `[post]` setting win over the look's, and a sky from the look is left out of saves.
  - **Soft Overcast:** the sky the main light, a weak warm sun with soft shadows, light shafts in thin blue mist,
    bounced light, auto exposure in a narrow range, AgX with saturation 0.9, a teal-and-warm grade, low wide bloom.
  - **Storybook:** a warm, strong sun pushed toward yellow-green against a teal sky, shadows tinted warm purple,
    teal fog in layers, tinted ambient occlusion, gold-tinted bloom, saturation 1.15, shadows lifted toward teal
    and highlights toward gold.
  - Still to come for them: a dappled canopy cookie, probes, and depth of field on the near foreground.
- **GI v2:** the meshes near the camera drawn into the voxels each frame, flat along each axis, as colour,
  normal and glow, and each voxel lit as its surface is (the sun through its shadow map, lamps, its glow, and last
  frame's bounce), so walls behind the camera bounce their own colour. Coarser grids are made from finer ones,
  keeping how much they block along each axis, so floors do not swallow light passing above them. Probes are ambient
  cubes traced with 32 cones, skipping those behind walls; reflections trace a cone through the voxels; mist is lit
  by the probes, and under the atmosphere takes its horizon's light. About 0.75 ms at 1440p. GPU tests: a wall the
  camera cannot see reddens the floor; torchlight bounces.
- **GI without pops:** probes refreshed in a scrambled order, so light changes evenly rather than in waves; every
  probe traced for the first frames, and exposure snapped meanwhile, so a level opens settled; a coarse cascade of
  probes four times as far apart past the fine one, so far walls and mist do not change over as the camera walks;
  voxels keep which way their light faces, so thin walls do not glow on their shaded side; exposure eases towards
  its target. About 1.1 ms at 1440p.
- **Auto exposure as cameras do it:** a 64-bin histogram of the picture's brightness, centre-weighted, averaged
  between its 70th and 95th percentiles, so sunlit parts are exposed well and shadows stay dark; moving steadily
  towards its target and easing in over the last stop and a half, at 3 stops a second towards light and 1 towards
  dark, as Unreal's does. Tested on a closed room with a doorway onto a sunlit yard.
- **Ray-traced GI probes (DDGI), as RTXGI in Arc Raiders and The Finals:** where the GPU has ray tracing (Vulkan
  ray queries: NVIDIA RTX 20 on, AMD RX 6000 on, Intel Arc), a bottom-level acceleration structure for each mesh
  and a top level per view and frame. Two cascades (three since the fix below) of 32×16×32 probes trace 256 rays each when due (128 until the flicker fix below), shading hits
  with the sun through a shadow ray, lamps, glow and last frame's probes; each keeps 8×8 octahedral irradiance and
  depth maps, and shading weighs probes by Chebyshev visibility, so no light passes walls. Probes inside geometry
  are set aside and probes near surfaces moved off them. GPU tests: a sealed box in sunlight stays black; a
  hidden red wall bounces. About 1.3 ms on an RTX 3070.
- **Ray-traced GI on every GPU:** without ray tracing, the probes' rays are traced in a compute shader, through a
  binned-SAH BVH over each mesh's finest LOD built when it loads and one over the frame's instances built each
  frame, nearer child first; each probe is traced half as often. `vox render --software-rays` forces it. GPU
  tests: the sealed box and the hidden wall, traced in software. About 3 ms in Sponza on an RTX 3070.
- **Voxel GI removed:** the ray-traced probes light every scene on every GPU, so the voxel clipmap, its radiance
  cache and their passes are gone. `[post.global_illumination] probe_spacing` (1 m) replaces `voxel_size`, and
  `ray_tracing` is gone. Fog, reflections and exposure's warm-up read the probes.
- **Glass:** a material setting, `glass = true`, with `ior`; `builtin:glass` is clear glass in every project.
  Glass instances are sorted after the opaque ones, past what culling, shadows and the GI's rays see, farthest
  first. After the sky, the picture is copied into an image with seven mips, each filtered down from the one before
  with Jimenez's thirteen-read tent, and each pane is drawn over it: what lies behind, moved by the normal's tilt in
  view, blurred through the mips by roughness with a ring of B-spline reads, so a lamp behind frosted glass is a
  soft glow rather than a square,
  tinted by the colour and dimmed by Fresnel, plus the surroundings, the sun and the lamps reflected. The editor's
  material inspector has Glass and IOR, and the Material menu lists the built-in Glass. GPU tests: clear glass
  shows what lies behind it, tinted glass colours it, rough glass blurs it.
- **Light through glass, and glass through glass:** every pane is drawn into a glass shadow atlas from the sun's
  cascades and the local lights' views (1024×1792: what it lets through, multiplied pane by pane, the nearest's
  depth, and its smoothness, with five mips that average the light and keep the nearest). Shading, the fog's light
  shafts and the GI's and reflections' rays filter the sun and the lamps by it: green glass throws green light, and
  rough glass's light is read from the mip as wide as it spreads, a soft patch growing with distance behind it.
  Panes are drawn in up to six layers, each over a copy made after the layers behind it, so glass shows through
  glass. Its two texture indices use the frame's spare fields, 0 for none. GPU tests: clear and green glass's
  light, frosted glass's spreading past its outline, and green glass seen through clear.
- **Colour temperature on lights:** `temperature` in kelvin on every light kind, which `color` then filters, as
  Unity's Filter and Temperature; a black body's colour from Kim et al.'s Planckian locus fit, at a luminance of 1
  so only the colour changes. The light inspector's Appearance picks Colour or Filter and Temperature.
- **Haze and a golden hour look, after Arc Raiders:** `[sky]` `haze`, `haze_tint`, `haze_glow`, `distance_haze`
  and `sun_size` set the atmosphere's aerosols, their forward scattering, the aerial perspective's scale and the
  sun's disc. `[post] look = "golden_hour"`: twice the haze, slightly warm, glowing tightly round the sun; thin
  volumetric dust with strong forward scattering; a wide warm bloom; AgX with more contrast and saturation, shade
  lifted a little to teal and highlights to cream; warm brown shadow tint. Film grain is removed: it never helped.
- **Lighting settings assets:** `.lighting` files (`AssetKind::Lighting`) hold a look to start from and `[sky]`,
  `[sun]` and `[post]` over it; `builtin:golden_hour` and the other looks are built-in lighting. A scene names one,
  `lighting = "..."`, which lights it alone as it loads (its own `[sky]` and `[post]` only while it names none), and
  written back on save. C#: `LightingSettings` asset fields and `Lighting.Apply`, which replaces the scenes' lighting until the
  level changes (native API 37). The Project panel's New Lighting Settings; the Inspector shows the file; an open
  scene re-lights when its lighting file changes.
- **The Lighting page:** a sun on the rail opens it in the drawer, as Unity's Lighting window: the open scene's
  lighting picked from None, the built-in sets and the project's, and a tab each for Sky, Sun, Exposure, Colour,
  Bloom, Fog, Shading and Lens, 57 settings showing their values in effect (the look's under the file's). A change
  is written into the asset with toml_edit, comments kept, and lights the scene at once; an emptied number goes
  back to the look's. Built-in sets are read-only with Make a Copy; a scene with none gets New Lighting Settings,
  carrying its own sky and post over. Colours open the colour picker. Editor test: pick an asset, change its
  haze, copy a built-in set, and save the scene naming it.
- **Seeing in the dark:** auto exposure leaves dark scenes darker the darker they are, at most three stops, and
  where the scene or a pixel is dark, colour fades to grey-blue (night vision), so moonlight reads as night. GPU
  tests: a courtyard from dawn to midnight and by torchlight.
- **A night sky:** a directional light's sky body, sun or moon. Under the moon the sky is drawn as dark as a
  night sky, however bright moonlight is made to see by, with the moon's disc and its seas; stars, scattered on a
  grid round the sky, each show where it outshines a few hundredths of the sky behind it, so none by day and the
  brightest first at dusk. The moon and the stars are drawn as the dark-adapted eye sees them, whatever the
  exposure. `[sky] stars` scales them. GPU test: stars and the moon under the moon, none in a day sky.
- **Clouds:** `[sky.clouds]` (`Sky::clouds`): cover, height, thickness, size, density, drift and shadows, with a
  Clouds tab on the Lighting page, and clouds to suit in every built-in look but Candlelit Castle. A layer between
  two heights over the level's ground, shaped from the smoke's baked tiling noise: round puffs for the bodies,
  smaller puffs and swirls bitten out of their edges, more cloud here and less there from a noise hashed on whole
  numbers, and one flat underside for them all.
  - **Clouds** pass: the layer marched along each pixel's ray at half size, 40 steps from a start jittered each
    frame and 4 towards the sun from each, blended with last frame's picture where that part of the sky was then.
    Lit by the sun through the air and by the sky, hazed with distance, laid over the sky by the sky pass with the
    sun, moon and stars behind.
  - **Cloud shadows** pass: a 256² map of the sun let through, 6 km each way round the camera, read wherever the
    sun's shadow is (`sunShadow`, `sunShadowHard`), so cloud shadows cross ground, blocks, plants, water, smoke and
    particles, and the fog's shafts fall between clouds. Under the atmosphere the sky's light greys with cover.
    The map's texels, 47 m wide, are on a grid that drifts with the clouds (`clouds::shadow_middle`), so a shadow
    glides over the ground; on a grid fixed to the ground it crossed it a texel at a time, in jumps.
  - **Clouds built of cubes:** `kind = "blocks"` (`CloudKind::Blocks`) with `block`, a cube's width. A cube is
    cloud or not as its middle is, so the shadow map's shadows are square too; the Clouds pass walks the cubes
    along each ray, 128 at most, out to 16 km, to the first one's face, and lights the face all one colour by
    where it faces and the cubes between it and the sun. The ray's place within its pixel moves each frame, so the
    blend with last frame's picture smooths the cubes' edges. `softness` turns the cubes to mist: each takes out
    only some of the light, less the nearer its cloud's edge, the walk goes on through them, and they are lit
    through rather than a face at a time.
  - The clouds drift along the Wind Zone's direction, and keep their place as the world's middle moves.
  - At 1080p on an RTX 3070, both passes together: 0.7 to 0.85 ms for soft clouds, 0.5 ms for solid cubes and
    0.7 ms for soft ones; nothing with no cover. GPU tests: cover fills the sky by its amount and is the same each run, clouds show in a gradient
    sky and under the moon, shadows put the ground in light and shade and move on as the clouds drift, cubes have
    flat faces and shade the ground, and the cost at 1080p. Editor and file tests: the Clouds tab's settings are
    written under `[sky.clouds]` one at a time over the look's.
  - Later: clouds in front of far ground and round a camera inside them, clouds in reflections.
- **Weather:** a `Weather` resource (`voxear_render::weather`) laid over the level's lighting as each frame is
  drawn, so the lighting is never changed and clear weather is the lighting as it is. Six amounts from 0 to 1:
  cloud fills the sky's clouds towards overcast; darkness thickens them, darkens their shadows and takes the sun's
  light; fog thickens the fog (turning it on) and the haze; wind strengthens the Wind Zone for plants, particles,
  the clouds' drift and a sea that follows it; rain and snow fall round the viewer; lightning strikes.
  - **Kinds:** Clear, Cloudy, Overcast, Rain, Storm, Snow, Fog, each a set of amounts, and Custom. Setting one
    with a time eases every amount from what it is now, by time passed.
  - **Rain and snow** (`voxear_engine::weather`): the starter effects over a box 24 m wide that stays over the
    viewer, in the world's space, as four Particle Systems each on Objs of the engine's own. As many are on as the
    amount reaches into; one stopped lets its drops finish and is taken away as a played effect is.
  - **What stays:** rain soaks every surface in 25 s and it dries over three minutes; snow settles on what
    faces up in 90 s, in patches first, and melts in four minutes, or in 40 s of heavy rain
    (`WeatherGround`, `weatherSurface` in `shading.slang`, for meshes, terrain and blocks alike). Weather a level
    starts in has already settled.
  - **Lightning:** strikes at times worked out from the weather's own clock, so the same weather has the same
    lightning; each two or three flickers that light the sky's light, the clouds and a gradient sky's colours.
  - **Lighting settings:** `[weather]` with a `kind`, or `custom` and amounts; applying lighting sets the weather
    to it, or to clear. A Weather tab on the Lighting page, the amounts shown for Custom.
  - **Scripts:** `Weather.Set(kind, seconds)`, `Weather.Set(amounts, seconds)`, `Weather.Kind`, `Weather.Amounts`,
    `Weather.IsTurning`, `Weather.Wetness`, `Weather.SnowCover`, `Weather.Flash`, and the events
    `Weather.Changed` and `Weather.Struck`; `WeatherKind` and `WeatherAmounts`. The script API is version 79.
  - Tests: easing and what each amount does; soaking, drying, settling and melting; lightning's times; lighting
    files; the page's rows; a C# system turning the weather, reading it back and hearing it change; GPU pictures
    of overcast, storm and fog against clear, of rain and snow falling, of wet soil, first snow and lying snow,
    and of a strike.
  - Later: rain and snow kept out from under roofs, bolts of lightning, sounds, the same weather on every machine
    of a networked game and in saves, weather by climate in generated worlds.
- **Look packs:** `.lookpack` files (`AssetKind::LookPack`, `voxear_engine::look_packs`): a `description` and the
  `[sky]`, `[sun]` and `[post]` of a `.lighting` file, with `[blocks]` for every Block World's small cubes, laid
  over whatever lighting a level has rather than in place of it. The level's own sky, sun look and
  post-processing stay in the world; each tick `look_packs::update` lays the pack worn (`Wearing`) over them as
  they are then and leaves the result as the renderer's `WornLook`, which a frame is drawn with, worked out again
  only when either changed. A pack that names a `look` starts from that built-in look instead. The weather is the
  level's: a pack cannot set it.
  - **Where packs come from:** six built in (`builtin:sunlit`, `cube_clouds`, `little_cubes`, `painted`,
    `old_film`, `plain`), the project's assets, all of which ship in a build, and a player's own files in `Looks`
    beside the game's saves (`file:` and a path). `look_packs::list` gives them all.
  - **The choice is kept** beside the saves (`look.txt`, as the language is) and worn as a game starts, in a build
    and when the editor plays.
  - **Scripts:** `LookPacks.All`, `Current`, `Use(pack, remember)`, `UseNone`, `Folder`, `Changed`; `LookPack`
    with its `Name`, `Description` and `Source`, the built-in ones by name, and as a field picked in the
    Inspector; `LookPackScreen.Open()`, a ready-made menu down the screen's left. The script API is version 81.
  - **Editor:** New Look Pack in the Project panel; the Inspector's description and what a pack changes over its
    text; **Look pack** on the Lighting page, which shows the scene under a pack and saves nothing; a pack saved
    from a text editor is worn again at once.
  - Tests: laying packs over looks (what changes, what stays, a level with no sun look, a pack's own look,
    settings there are not), every built-in pack, the worn look following the level's lighting, a player's
    folder; a C# system listing, wearing, trying and taking off packs, and the game starting in the one kept; a
    GPU picture of a Block World under each built-in pack with the level's own lighting unchanged.
  - Later: water in a pack, pictures of a pack's own (a colour lookup table, block pictures), a pack's own
    shader over the picture, a rows-and-tabs Inspector for a pack as lighting settings have.
- **Cut-out materials in the rays:** the GI's and the reflections' rays pass through the holes of an alpha-tested
  material, as the depth pass discards them: a cut-out instance is in the top level as not opaque, and a ray query
  keeps each of its triangles only where the base colour's alpha reaches the cutoff; the software tracer tests
  them the same way. A ray finding the back of a sheet (cut-out or foliage) no longer marks a probe as inside a
  wall. GPU tests: a wall cut away all over bounces nothing, a solid one bounces its colour, both tracers.

Next:
- Reference look: The Finals and Arc Raiders (Embark), for bounced light, exposure and contrast.
- Probe rays for reflections.
- Virtual shadow maps.
- Optional third-party upscalers: AMD FSR, NVIDIA DLSS and Intel XeSS, each behind its own SDK.
- Baked lightmapper, light probes and reflection probes.
- Reflections: falling back to reflection probes rather than the sky, and tracing against the depth pyramid,
  for fewer misses on surfaces seen edge on.
- **Ray-traced audio** as an optional setting: GPU rays against the GI's acceleration structures, fed to Steam
  Audio, with baked reverb probes as the fallback.
- **HDR colour, end to end.** Rendering is HDR already: a float target, emissive and lights above 1, exposure and
  tonemapping, and script colours go past white with `[ColorUsage(hdr: true)]`. Still to come:
  - HDR textures (`.exr` and `.hdr`) imported as half floats and cooked to BC6H, for skies, light probes and
    emissive maps
  - HDR display output: HDR10 and scRGB swapchains with paper-white and peak-brightness settings, and tonemapping
    that targets the display

## Vegetation ✅ (done)

What The Vegetation Engine brings to Unity, built in: foliage that is cheap to draw in great numbers, moves with
the wind and with what passes through it, and changes with the seasons, from one set of settings per level.
Foliage shading (wrap lighting, light through the leaves, a rim of the sun's colour) is done, in M5.

- ~~**Drawing a lot of it:**~~ done: a Foliage component (mesh, material, plants, draw, fade and shadow distances)
  with plants placed (`[x, y, z, turn, size]`) and scattered (`size`, `density`, `seed`, `scale`: jittered grid,
  SplitMix64). Each view builds a Foliage's instances once, in 16 m chunks, rebuilt only when its plants' list,
  Obj, mesh, material or distances change, and adds the chunks within draw distance to the frame's instances, so
  GPU culling, occlusion, LODs, meshlets, wind and pushes apply as to any mesh. Instances gain `fade` (dithered
  out by distance in every fragment pass) and `INSTANCE_NO_SHADOW` (chunks past the shadow distance). Instances
  are owned by (Obj, plant), so plants keep their LOD history and skip motion tracking. Scene files keep the
  scatter rather than its plants (`FoliageSource`), headless too. Editor: Add Component makes a starter Foliage;
  distances in the Inspector, undoable. GPU test: 12,800 plants drawn out to their draw distance and no further.
  About 12,000 plants in the valley test: 6 ms a frame at 720p on an RTX 3070. Plants are tinted between a Foliage's `healthy_color` and `dry_color` in patches
  `color_spread` metres across (packed RGBA8 in an instance's `fade.z`). Clicking a plant in the Scene view picks its Foliage
  (each plant's box); placing passes plants by. Scattering onto uneven ground moved to M8, with the scatter brush.
- **A speed pass, measured:** in the outdoor valley at 1080p on an RTX 3070, the passes added up from 8.0 to
  5.4 ms. `vox render` averages each pass over the last 60 frames and prints the frame's own GPU time, first pass
  to last (`GpuTimer::frame_ms`, `Renderer::gpu_frame_ms`), as single frames varied by a millisecond and
  passes overlap; the stats overlay's CPU line is the frame less its waits for the GPU and the display
  (`Renderer::waited_ms`), not the time between frames, which vsync pins at 16.7 ms. Then:
  - GI probes with nothing within a spacing **sleep** (DDGI's probe states, Majercik et al. 2021): traced eight
    times less often; the update fell from 0.86 to 0.29 ms. (Reading the rays through shared memory in the
    update made its loop five times slower on the RTX 3070, so only relocation does.)
  - Shading reads the coarse GI cascade only where the fine one does not wholly cover a point, each probe's
    state once, and no GI reflection on surfaces rough enough to reflect only the ambient: 2.7 to 1.8 ms.
  - Plants of a Foliage are left out of the GI's rays: rays 0.79 to 0.36 ms, the scene's build 0.21 to 0.10.
  - Shadows: casters under a metre across draw only into the cascades they stand in or border, as their
    shadows are short; casters under two texels of a cascade not at all. 0.94 to 0.30 ms.
  - Under the atmosphere, the sun's light through the air and the sky's ambient are worked out once a frame
    (**Sky light**, a one-thread pass) instead of five sky lookups a pixel: shading 1.64 to 1.44 ms.
  - Foliage's leaf shadow uses the hard shadow, and surfaces turned from the sun skip the shadow.
  - Fog's light from the probes finds each probe's visibility once for its looks up and down: 0.22 to 0.15 ms.
  - LODs past a mesh's seams: where meshoptimizer's attribute-aware simplification stalls (over 1.25 times its
    target) or its error jumps eightfold, the LOD is made by sloppy simplification instead (error scaled to
    absolute by `simplify_scale`), each LOD's error held at least the last's. The built-in sphere now goes 4,096
    down to 109 triangles rather than stopping at 512; the meadow's meshlets fell from 40,000 to 8,000, the depth
    prepass 0.35 to 0.11 ms, shading 1.44 to 1.06 ms. `COOKED_VERSION` 5. The valley's frame: 3.1 ms at 720p,
    4.2 ms at 1080p, 5.8 ms at 1440p.
  - Measured and left: GI sampling in shading is 0.46 ms at 1080p, near what eight probes a pixel cost; reading it
    at half resolution would lose normal maps' detail in shade. The depth prepass, which ambient occlusion,
    contact shadows and reflections bring, saves 0.75 ms of shading overdraw on its own.
  - The ray tracing scene's top level is built only when its instances or the meshes' bottom levels changed since
    that frame slot's last build (`Assets::bottom_level_changes`), so a still level skips its build and the
    barrier after it: the valley 4.2 to 3.8 ms at 1080p.
  - The coarse GI cascade refreshes every sixteenth frame rather than every fourth: it lights only the distance,
    where a change shows late anyway, and the fine cascade still lights what is near (a carried torch).
  - The depth prepass writes each pixel's roughness to an R8 target, so screen-space reflections skip pixels too
    rough to reflect without reading their material: 0.17 to 0.05 ms.
  - Fog integration is a compute pass, a thread a column of froxels walking its 64 slices once, rather than a
    pixel a froxel adding up every slice in front of it again: 0.11 to 0.02 ms. The valley: 3.6 ms at 1080p.
- **Ray-traced reflections:** where the GPU has ray tracing and the GI traces its scene, the reflections pass traces
  each smooth pixel's mirror ray through the GI's top level (`rayReflectionsFragment` in `shaders/ddgi.slang`, half
  resolution, built only with the hardware variant) and lights what it meets as the GI's rays do, textures at a mip
  by distance; misses take the sky. The depth passes' R8 roughness target became `SURFACE_FORMAT`
  (A2B10G10R10: an octahedral normal, normal map and all, and roughness), as depth-rebuilt normals faceted a chrome
  ball's reflection. 0.13 ms on the Sponza demo's chrome ball at 960×540. Screen-space reflections stay for GPUs
  without ray tracing.
- **Bounced light that goes out with its lamp:** a probe texel's blend keeps less of its last value the more its
  light changed (`DDGI_CHANGE`, RTXGI's irradiance threshold), and far less when it fell (`DDGI_FALL`, 12 to 30%):
  each ray's hit takes light from the probes' last values, so in a pale room the bounce fed itself and lingered for
  seconds after its lamp went out. A grey room's bounce is gone within a third of a second; a lit room's still
  light is within 0.1% of before, with no visible flicker. The blend follows each texel's running change
  (`DDGI_TREND`, kept in the light's alpha), not one frame's: in Sponza, lit by candles, a frame's rays swung past
  the thresholds and the walls flickered (each pixel's spread over 120 still frames 1.37 levels, against 0.83
  with a fixed blend). Only falls are taken quickly: a rise taken at once fed itself as a fall did, and the
  corridor round the demo's neon tube surged to twice as bright for eight frames at a time. Each ray's light is
  capped (`DDGI_RAY_MOST`, 16 by its brightest channel): the tube lit its own glass from 4 cm, and whether a
  probe's rays met that sliver changed as they turned and as probes moved. Over 900 editor frames of the corridor
  the metered brightness now stays within 3% (it swung fourfold), and still frames spread 0.83. Then the walls by
  the neon tube still shimmered: the noise left in each probe update. Measured as each pixel's spread over 60 still
  editor frames at one exposure: 0.37 without GI, 0.42 with the probes' light frozen, 0.52 at 128 rays and 0.95,
  0.45 at 256 rays and `DDGI_KEEP` 0.97 (chosen: 0.44 ms more at 1080p; 128 rays at 0.98 gave 0.46 for nothing but
  let new light in far slower). Probes held still, no visibility test, the smooth normal, and fog or reflections
  off each changed nothing; the few edge pixels that flicker hardest are there without GI too. Those went with TAA
  keeping more of last frame by default (`taa_history` 0.9 to 0.97, eased off as before where things move fast):
  pixels flickering hard 57 to 17, the spread 0.45 to 0.36, under what GI-less frames showed at 0.9. And the
  probes' light is kept and blended raised to 1/5 (`DDGI_GAMMA`, as RTXGI does, Majercik et al. 2021), so a few
  rays that meet a bright sliver move a probe little: the spread 0.36 to 0.35, the picture 1% darker.
- **Shadow distance in the lighting:** `[sun] shadow_distance` (80 m by default, 5 to 1000) and `shadow_split` (0.85,
  0 even to 1 logarithmic) in lighting files, on `SunLook`, and on the Lighting page's Sun tab as Shadow distance
  and Cascade split; the four cascades fit them (`shadows::cascades`). Unit tests for the cascades and the file.
- **Reflections at full resolution:** the reflections pass traced half resolution, each 2×2 from a different one of
  its pixels each frame, as GTAO does, but nothing blended them, so a curved mirror's reflection changed every frame by
  more than TAA's clamp lets through: the chrome ball showed dots cycling with the jitter when the camera stopped (108
  of its pixels, 2 without reflections). Blending them over frames at full resolution (reprojected by the surface,
  then by the reflected point, as far behind the mirror as the ray went) cut that to 3 but, moving, the reflection
  still broke into blocks and smears. Now every pixel traces its own ray (`level` 0): 5 such pixels still, crisp
  moving; 0.98 ms at 1080p in the Sponza demo, against 0.36 ms at half resolution. A reflection that meets the
  back of something shows it lit as its front, as the GI's rays already light it, rather than black: curtains and
  cards of ivy are sheets, and black there left dark patches over the chrome ball (thousands of pixels a frame, found
  by painting them magenta). One that meets the inside of the surface it left, within 5 cm (`REFLECTION_SELF`), as
  from a coarse LOD's facet just inside the full one the rays are traced against, carries on past it.
- **Mirrors that do not crawl or streak:** a reflection's textures are read at the mip its pixel's ray cone covers
  where it lands (Akenine-Möller et al. 2019): the pixel's width on the surface and how fast the reflection turns
  from pixel to pixel, from screen derivatives, times how far the ray went (`REFLECTION_TEXELS_PER_METRE` 512). Read
  near full detail, the floor's grout and stains broke into dark dots and lines over the chrome ball that crawled as
  the camera moved. And TAA keeps less history on mirrors while the camera moves (`mirrorKeep` in `post.slang`: down
  to 0.4 of it, by the depth passes' roughness and `post.camera_motion`): its history follows the surface, but a
  reflection moves as what it shows, so a quick move dragged the old reflection across the new in dark streaks.
  Still, mirrors keep it all. The surroundings a surface reflects, from the probes where no traced or screen-space
  reflection covers it and blended under one by roughness, are looked up a probe's spacing out from the surface
  rather than half a spacing towards the camera: from just off the chrome ball, the probes round it were inside it
  or had it in their way, and dark smudges spaced like the probe grid moved over it as the camera did. The same
  held where a traced reflection lands: its probe light is looked up straight out from the surface it met, not back
  along the ray towards the ball (`ddgiShadeHit`'s `fromProbe`; a probe's own rays still look back towards it).
  And up to half the roughness past which reflections stop, a surface shows the traced or screen-space reflection
  alone, fading to its surroundings only over the rest: a chrome ball (roughness 0.05 of 0.6) took 8% of the
  surroundings' blotchier probe light, enough for a dark smudge over its reflected curtain.
- **GI that does not change as the camera walks up:** three cascades of probes, each twice as far apart as the last
  (`DDGI_CASCADES` 3, `DDGI_CASCADE_SCALE` 2), rather than two four times apart. Past the fine grid's sixteen metres,
  probes four metres apart smeared the neon tube's light down Sponza's corridor, and it changed before the camera as
  the fine grid took over (a probe spacing of 2 moved the change out, which showed the cause). Shading blends from
  the nearest covering cascade outwards; the middle one refreshes an eighth of its probes a frame. Same reach, to
  64 m; 0.29 ms more at 1080p (rays 0.57 to 0.80 ms, update 0.38 to 0.43 ms); flicker unchanged.
- **A model as one Obj:** a model of several parts placed in a scene is one Obj with a Model Renderer (`ModelRenderer`
  in voxear_render: shared parts, each a mesh, material and place in the Obj's space), not an Obj per part. Each part
  is an instance of its own in extraction (`Owner::part`, motion tracked per part), so culling, LODs, shadows, the
  GI's rays and reflections take it as any mesh; picking tries each part's box. Scene files write `[obj.model]`
  (`ModelData`, kept as `ModelSource`); a reimport rebuilds the parts; in-use counting keeps its meshes, and Foliage's.
  Inspector: Model (picks another model), LOD bias, Parts, and **Unpack to Objs** in its menu, one undo step.
- **No NaN from a missing normal:** `normalizeOr` in `shading.slang`; a zero-length normal or tangent (some of
  Sponza's window frames, vases and plants) made NaN that bloom spread over the whole picture.
- ~~**Cheaper far away:**~~ done.
  - ~~LODs that fade into each other by dithering rather than popping, and grass and small plants that fade out
    by distance~~ done: `ditherLodFade` in `shading.slang`, and a Foliage's fade distance.
  - ~~Impostors for far trees~~ done: the first time a Foliage's plants are far enough, `impostors.rs` bakes the
    plant's LOD0 into a 1024² atlas of 8×8 hemi-octahedral pictures (colour, and normals in the plant's space),
    each an orthographic view of its bounding sphere, and makes a material of it (`MATERIAL_IMPOSTOR`). Past the
    distance a plant's instance becomes one on a two-sided quad (`INSTANCE_IMPOSTOR`, no shadow, left out of the
    GI's rays) that `shaders/impostor.slang` turns to the picture taken from nearest the camera's side, lit by its
    baked normals with the plant's tint and seasons. Foliage's `impostors`: `auto` (plants 2 m across or larger,
    from 40 m or a third of the draw distance), `off`, or `from` `impostor_distance`; Inspector, scene files. GPU
    test: a leaning spire and its impostor cover the same pixels, lit alike.
  - ~~Shadows for foliage drawn from cheaper LODs~~ done: plants (a Foliage's, or anything with a foliage material)
    pick their shadow's LOD at four times the error (`PLANT_SHADOW_COARSER`); a Foliage's shadow distance already
    leaves far grass without one. The valley's shadows 0.25 to 0.19 ms, their meshlets 6,700 to 3,000.
- ~~**Wind:**~~ done: a Wind Zone component (strength, gusts, gust size and speed, turbulence) blowing along its
  Obj's +Z over the level. `shaders/wind.slang` moves every vertex in the forward, depth, shadow and mesh-shader
  passes, and last frame's for motion vectors: plants bend from their pivot up by height (from the mesh's
  bounds) and lean downwind on an arc, sway at a phase of their own, and their leaves flutter along their
  normals; gusts are two octaves of value noise drifting downwind, so a field shows waves. Materials have
  `wind_bend` and `wind_flutter`, 1 for foliage unless written; culling widens a moving plant's sphere. Editor:
  Wind Zone component with arrows; material inspector Wind bend and Leaf flutter. Masks for branch sway apart
  from the trunk came with converting plants, below.
- ~~**Pushing through it:**~~ done: a Foliage Interaction component (radius, strength). The 16 pushers nearest the
  camera go to the frame; a persistent 128×128 grid of half-metre cells round the camera (toroidal, as the GI's
  probes) holds a push each, and **Foliage pushes** springs every cell back (to a third in 0.8 s) and stamps the
  strongest push outwards from each pusher. `wind.slang` reads it at each vertex, blended over four cells, and
  bends the plant away from the ground up, on an arc, with or without wind. GPU test: a pole leans from a pusher
  beside it and stands again once it has gone.
- ~~**Plants of cubes, leaves of blocks, and characters:**~~ done.
  - A box model has a `wind_bend` (the Model window's Inspector, under Model; `MaterialDesc::wind_bend` on its
    material): it bends in the wind and from pushes as any plant, with no flutter, which would part its cubes'
    faces, and masks of its own (`sway_masks`) that sway what reaches out from over its foot. The starter plants
    have theirs: a tuft 2, a flower 1.5, a bush 0.8, trees 0.25 to 0.35, a rock 0.
  - A block **sways** where its `.block` says so or one of its materials bends in the wind (`BlockKind::sways`,
    read where the world is resolved, as `cutout` is). The mesher keeps such a block's faces a block each and
    marks, in four spare bits of a quad's first word, which corners no still block touches
    (`mesh::swaying_corners`); `blockSway` in `shaders/blocks.slang` moves those, by the gust over them and at a
    phase hashed from the corner's own whole-number place, in the shaded, depth and shadow passes, with last
    frame's for motion. Every face at a corner moves it alike and held corners stay, so nothing parts. Only the
    world's first level sways. The face's picture and its bounced light are read where it stands.
  - Every Obj with a Character Controller pushes foliage with nothing set up (`voxear_engine::walkers` notes them
    in `voxear_render::Walkers` each tick; a Foliage Interaction of its own takes its place).
  - The weather's wind blows in a level with no Wind Zone (`WeatherAmounts::wind_alone`), along +X as clouds
    drift there, for plants and particles alike.
  - **Solid plants:** a Foliage's **Collider** is None, Trunk or Box (`Trunk::solid`, `box = [x, y, z]` in a
    scene's `[obj.foliage.trunk]`): a box standing on the ground, sized, turned and leaning with each plant, for
    rocks, stumps and crates. Picking Box fits it to the plant's model. The starter rock has one.
  - A cooked model keeps its materials' wind bend and flutter (`COOKED_VERSION` 13), so a built game's plants of
    cubes bend too.
  - A block's face drawn with its colour alone is matt (roughness 1, where it was the default 0.5 and shone).
  - Tests: the mesher's free and held corners; GPU pictures of a pine leaning in a wind, a tuft pushed over by a
    walker, a rock that does neither, and a tree of blocks whose leaves stir while its trunk and ground stay
    still; a ray meeting a rock's box, turned and sized; the Model window's Wind bend typed, saved and undone.
  - Later: a breeze on a clear day with no Wind Zone, block leaves pushed aside, a collider of a plant's own
    cubes rather than a box, plants of an earlier project given their bend by `vox upgrade`.
- ~~**Seasons and variation:**~~ done: a Seasons component (the level's first) with `time_of_year` from 0 to 4
  (spring, summer, autumn, winter, spring), each season's look (`color`, `color_amount`, `snow`), `variation`,
  `patch_size`, `dryness` and `dry_color`, and `wetness`; defaults leave plants as made at summer.
  `shaders/seasons.slang`: per plant, by noise at its origin, the year runs up to half a season early or late, and
  leaves turn redder or yellower and dry in patches; a season's colour replaces the material's, keeping its
  texture's light and dark. Plants (foliage, or anything with wind) take snow from above and darken and shine when
  wet, in the depth pass's roughness too. Materials' `leaves`: `turn` or `stay` (pines), `MATERIAL_LEAVES_STAY`.
  (Leaves first fell a triangle at a time towards winter; it read as broken meshes, so leaves stay on.) Editor: Add
  Component, the Inspector showing the look of the season the year is nearest under a Season choice, undo, copy
  and paste, prefabs; a Leaves choice in the material Inspector. C#: `Seasons.TimeOfYear`, `Seasons.Season`,
  `Seasons.Exists`. Seasons reach the GI's rays and ray-traced reflections (`ddgiShadeHit`), and `ground_cover` lays
  the snow and wetness on what is not a plant.
- ~~**Glow:**~~ done: materials' `glow` (`steady`, `pulse`, `breathe`, `flicker`) and `glow_speed` move their emission
  (`shaders/glow.slang`, two bits of the material's flags and its spare float), each Obj or plant at its own moment by
  where it stands; the GI's rays see the same, so the light glowing plants bounce moves with them. Fields that do
  nothing hide in the material Inspector (glow speed, leaves, IOR).
- **Placing it:** moved to M8, Terrain, to be built with terrain's scatter layers.
- ~~**Converting plants:**~~ done: `Vertex` gains `wind` (four bytes: leaf flutter, phase, branch sway, unused;
  `NO_WIND_MASKS` moves a plant as before), so 52 bytes, `COOKED_VERSION` 6. A model's Import Settings choose
  **Wind masks** (`ModelImport::wind_masks`): none, from vertex colours (glTF `COLOR_0`, FBX vertex colours;
  CryEngine and Unreal's red flutter, green phase, blue branch), or generated from the shape
  (`mesh_tools::generate_wind_masks`: pieces joined by triangles or shared positions; those under a fifth of the
  plant's size, not the widest, are leaves that flutter at a phase of their own and sway with the branch they hang
  from, by their centre's reach from the trunk; the rest sway by their own reach, eased in, without flutter).
  `wind.slang` scales flutter by the mask, offsets phases, and sways branches downwind and bobbing
  (`WIND_BRANCH`). Unit tests for generation and the settings; GPU test: a pole's masked top sways further
  downwind, its foot still. The foliage shading's needs (normals, foliage materials) were already met by import.

## Water ✅ (done)

The sea, lakes, rivers and lava, built in. Manual: `manual/Water`.

- **Ocean** (`voxear_render::ocean`, `water.rs`, `shaders/water.slang`): FFT over dual JONSWAP spectra
  (swell and wind sea), six cascades of 256² over tiles from 1031 m down to 1.13 m, each its own band of waves,
  summed by a groupshared radix-2 inverse FFT into movement, folding and slopes with mips. Weather presets (Calm,
  Normal, Stormy, Custom), Wave scale (taller without folding harder), choppiness, and a colour blend of deep water and
  light through the crests. A clipmap of 12 rings round the camera, morphing at their edges. Glints over slope-variance
  roughness, refraction and Beer–Lambert absorption over a copy of the scene and its depth, written to depth and
  velocity. Changing Wave scale or choppiness rebuilds nothing, as both are applied as the waves move; a new
  spectrum (weather, winds) takes a few milliseconds, its cascades built on threads.
- **Shores:** the ground under the sea captured round the camera from above and below, in four slices, its depth
  blurred over a few metres. Waves calm by that depth, the longest first, so they ease off over a drop-off rather than
  stepping into a wall, and a rock in deep water barely calms them. Water is cut away where the ground reaches down
  into it, and carries on under overhangs. Waves break into surf where they are about as tall as the water is deep.
  Refraction bends the view under the water by Snell's law.
- **Foam:** a simulation round the camera in rest space, made where crests break and where water surges against the
  ground, drifting downwind, running up the shore and draining back, spreading, and ageing from fresh froth to lace.
  Its look comes from a generated pattern of gradient noise: froth in clumps of fine bubbles, lace in thin streaks,
  bubble domes, standing up off the water near the camera with self-shadowing, flat further off.
- **Under the sea:** `underwaterSun` dims and tints lit surfaces with a caustic web, the under-water pass fogs the
  view by distance, absorbing a colour at a time and scattering all of them, with light shafts, a waterline that
  follows the waves, and drops on the lens after surfacing; it runs after the air's fog, so no haze shows under the
  water. The surface from below shows Snell's window and total internal reflection.
- **Quality:** the Ocean's costlier looks (Shores, Foam Trails, 3D Foam, Light Bending, Light Shafts, Wet Lens, Fine
  Ripples) each turn off in the Inspector or from C# as `Water.Features`, skipping their passes.
- **Pools and Dry Areas** (`water_bodies.rs`): a Pool is water (or lava) over a box at its Obj's height, its walls
  and bottom drawn as walls of water, below the sea dry to it, with as much of the sea's swell as it says. A Dry
  Area (box, ball or capsule) is cut out of every surface per pixel, its sides walls of water. A Dry Area shaped as a
  closed mesh, its own or its Obj's Mesh Renderer's, keeps a hull dry exactly: each frame the meshes are drawn into
  a mask of where they start and end along each pixel's view, which the water, the view under it and underwater
  lighting read.
- **Paths and Streams:** `voxear_scene::Path` (centripetal Catmull-Rom, widths per point), edited in the Scene view
  (`path_handles.rs`: drag, width, insert, extend, Shift-click to remove, one undo step each). A Stream samples its
  Path each frame into world points with Manning speed, rapids foam carried downstream and travel time; drawn as a
  strip, 16 quads across and four rows a point, with two-phase flow on the sea's finest cascades. Its look goes by
  how fast it runs: calm water near flat and glassy, its bed dappled with the caustics carried along with it and no
  foam on still shallows; faster water rougher and drawn out into streaks along the current; rapids raised into
  standing waves and churn (`streamSwell`, the strip and its shading alike), froth with broken edges and glints on
  their crests and clear green troughs, with fine spray the renderer adds as a particle system for each Stream with
  rapids (`streamSprayVertex`). What stands in fast water has a bow wave upstream, a white collar, and a wake that
  spreads downstream in a V with a trail of white down its middle, and water fast over a shallow bed breaks white:
  all found in the picture's depth behind the water, a few places straight upstream, out along each arm of the V,
  and just downstream (`streamWake`). Tests: rapids white and the level stretch not, calm smooth against fast rough, white
  water below a rock in fast water and none in calm, spray gathered only for rapids.
- **Lava:** Pools and Streams of lava glow by Planck's law: yellow-white only where it has just welled up, orange
  across a lead, deep red against the crust. A skin sets on molten rock in small cells with bright seams; crust forms
  in plates, each from its middle out, on a lattice of three ways so they meet three at a corner, their edges
  wandering. The plates stand above the molten rock between them, which is seen down into past the near plate's
  edge (`lavaInto`, followed across the plates as they are, round corners and where gaps join); gaps narrow as it
  cools to leads in some stretches and cracks in most, and crust breaks up in places into smaller pieces with dull
  red cracks between. Crust is thick and ropey: folded round each plate's rim, the rim proud, each plate leaning its
  own way. Molten rock heaves in slow swells (`lavaHeave`, the surface's points and its shading alike) and its skin
  folds against the crust. A flow stands as high as it is deep with rounded sides and ends, crusts from its banks in
  on a molten channel, its plates placed by when the lava set out, with pahoehoe ropes bowed at the banks. A lake
  turns over in wandering patches a few metres across. Bubbles swell into domes, break the crust, burst and leave a
  ring that heals; a burst is a small explosion, up to 144 drops and clots in a spread of sizes that cool as they
  fly; vents spit one burst after another. All of it is worked out from place and time, alike on every machine: its
  cells' numbers are hashed in whole numbers (`lavaCell`), since numbers taken from a sine differ from one use to the
  next and left the noise broken along its lattice. Lava lights what is
  round it with rectangle lights the renderer adds, without shadows. Crusted at once under the sea. Tests: molten
  channel and crusted banks, plates against cracks, bubbles and spatter over time and none on a crusted lake, a
  wall lit by a lake.
- **Queries and Buoyancy:** `voxear_engine::water` keeps a CPU copy of the two longest cascades (the same waves, on
  every machine) and the Streams' points, for `Water.HeightAt`, `NormalAt`, `FlowAt`, `IsUnderwater` and
  `TrySample` in C#. Buoyancy pushes nine columns across a body's Collider up by how far each is under, with drag
  against the water's own movement as its waves carry it round, so floating things ride a storm's waves. The CPU
  finds the sea floor by casting down onto Colliders, so ground needs Colliders for floating things to feel it.
- **Wind:** an Ocean's Wind Source can be the level's Wind Zone, whose arrow and strength raise its wind sea; a
  changed wind eases over the Ocean's Transition, a step every quarter second, alike on every machine, and each frame
  in flight has its own spectrum buffer, so a new one never waits for the GPU. A Sphere or Box Wind Zone makes the
  sea's short waves inside it as rough as its strength, a calm bay or a squall, fading out over its falloff.
- **Stream mouths:** from where a Stream of water reaches the sea or a Pool, it becomes that water over ten metres,
  its surface easing to the water's height and waves and fading in from its banks; into the sea, it pours out a jet
  of foam. `Obj.Buoyancy` gives scripts a floating Obj's settings.
- **Streams meeting:** a Stream of water ending in another that runs on past it eases into it as a tributary, the
  river churning white below the join; two ending in each other meet head on, fading into each other, both churning.
- **Bilges:** a Dry Area's Bilge is water standing in it, level as its hull rocks: a Pool over the box round it
  that keeps to the Dry Area's inside, with no walls.
- **Indoors:** the sea's floor counts ground only up to 4 m over its surface, so it runs on under roofs and arches;
  walls, drawn as lines from straight above as they show edge on, stop its waves and the blur of its depth; over flat
  shallows, which the waves broke before reaching, as a flooded floor, it lies near still; and it mirrors what is on
  screen round it, walked across the picture, before the probes' light.
- **Lamps:** their glints on the water and their light in its own colour; under it, their glow in the water along
  the view (each lamp's light added up exactly along it), and their light dimmed by the water on its way to what
  they light.
- **Spray and splashes:** an Ocean's Spray throws drops off where its foam is fresh, breaking crests and surf, as a
  particle system the renderer adds of itself (`sprayVertex`): each drop tries a few places in a square tiled over
  the sea round the camera, so drops stay put as it moves. A body that goes into the water fast enough, floating or
  not, throws up a burst of drops, as big as it is fast and wide, cleared once they have fallen (`water::splash`);
  one that ploughs through half in it throws up smaller ones as it goes; lava throws up blobs of molten rock.
  `Water.Splash` makes one from C#.

## M6 — Animation (in progress)

- [x] GPU skinning, and an Animator playing clips from any model file whose skeleton names its bones as the model's
  does, with root motion; a change of clip never pops (inertialisation)
- [x] motion matching: Motion Sets of states and their clips, the feature database, and a Motion Matcher that
  searches it for how the character is wanted to move and blends the best matches
- [x] Actions: clips played once over a state's, on the whole body or a part of it
- [x] rigs found on any skeleton (arms, hands, fingers, spine, head), and IK that reaches a hand, aims it and turns
  the spine
- [x] Bone Attachments, and bones read and posed from scripts after animation
- [x] the First-Person View: the camera in a body that is seen, with kick, glance and framing
- [ ] blend shapes, state machines, blend trees and layers
- [ ] GPU crowd search
- **Box models**, for the blocky characters of block worlds, in parts: the asset; a window to build and paint one
  in; bones made from its shape; motion made from how it moves; a raised second layer; walking in a block world;
  clips keyed on a timeline.
  - ~~**The asset:**~~ done: `.boxmodel` files (`voxear_render::box_model`: `BoxModel`, its `Sheet` of pixels kept in
    the file as a palette, compressed, and `BoxBone`s each with `ModelBox`es whose faces are unfolded on the sheet or
    placed one by one, mirrored, inflated, turned, or flat). `prepare_model` reads one into a `PreparedModel` (one
    skinned mesh, each box bound whole to its bone; the bones its skeleton; the sheet its texture), so it is
    `AssetKind::Model` and is placed, drawn, posed, previewed, imported again when its file changes and cooked as any
    model. Sharp pixels: `MaterialDesc::pixels`, `MATERIAL_PIXELS`, `sampleBaseColor`; `COOKED_VERSION` 11. Editor: New
    Box Model (a grey block on one bone), its kind and a Box Model section in the Inspector (Scale, Sheet, Bones,
    Boxes), previews showing its face. A model imported again gives the Objs drawing it its bones as they now are
    (`Game::reimport_model`), and a menu with more items than its window has room for keeps to the window and scrolls
    (`Ui::place_menu`). Tests: files read back, palettes, refused files, the unfolded layout, faces wound and mapped the
    right way round, boxes placed by their bones, flat and inflated boxes, the prepared model; the GPU drawing one with
    each pixel one colour to its edge, from source and cooked, and a box following its posed bone; the engine reading
    its bones headless, a scene's Obj getting them, and builds cooking it; the editor making one, showing it and
    following its file to a new bone; a long menu scrolled to its last item. Manual: Box Models.
  - ~~**The Model window:**~~ done: a `model` dock panel (`model_panel::ModelPanel`), opened by a double click on a
    `.boxmodel`. A tree of bones with their boxes, to pick from and drag onto one another; the model on a floor of
    blocks (`model_view::ModelView`), drawn as a preview with a camera of its own (`PreviewCamera`,
    `PreviewOptions::camera`, up to `MAX_VIEW_SIZE` 1024 pixels) and cut to the view's shape, the camera turned by a
    drag, moved by the middle button and brought nearer by the wheel; a click picks the box under it by a ray
    (`BoxModel::pick`), and what is picked is outlined with an arrow an axis and, on a box, a knob a face, which
    move and size it a whole pixel at a time. The bar adds a bone or a box (given free room on the sheet,
    `free_place`), duplicates, mirrors across the body (`mirror_bone`, `mirrored_box`), deletes, and with Pivot
    Only moves a bone's pivot alone (`move_pivot`). The part picked shows in the Inspector
    (`box_part_inspector::BoxPartView`, `Inspector::show_box_part`). Its own undo and redo while it has the keys,
    one drag one step; saved on let go, and Objs drawing the model follow (`editor/models.rs`). Tests: rays to
    box, face and pixel; bones removed, hung elsewhere, copied and mirrored with nothing moving; pivots moved;
    free room on the sheet; a view preview drawn where its camera projects; the view's rays and handles; the
    editor opening, picking, dragging a handle, typing a size and a name, adding, mirroring, deleting and undoing,
    with the file saved once let go. Manual: Box Models.
  - ~~**Painting:**~~ done: the Model window's Paint mode. A press on the model paints the sheet's pixel the ray
    meets (`BoxHit::pixel`), a drag every pixel passed over, and Symmetry the pixel across the body too; the pixel
    under the pointer is outlined on its face (`BoxModel::pixel_corners`). The sheet shows flat beside the model
    (`model_sheet::SheetView`), each pixel a square over a chequered ground, the faces' places outlined, painted
    the same way. Pencil, Eraser, Fill (`Sheet::flood`, no further than the face) and Pick; the colour from a
    picker or from the sheet's own colours (`Sheet::colors`); Lay Out (`BoxModel::lay_out`: every unfolded box
    given room afresh with its pixels, shared pixels kept shared, placed faces left, the sheet doubling while too
    small). A stroke is one step to undo; pixels alone changed upload the sheet behind the same material. Tests:
    fills, a sheet's colours, faces' places, laying out; the editor painting on the model and the sheet, a drag
    filling a row however fast, symmetry, fill, pick, erase, laying out and undoing each. Manual: Box Models.
  - ~~**Bones from its shape:**~~ done: `BoxModel::make_bones` (`box_model/rig.rs`) takes the box most others
    touch for the body, what stands on the ground under or beside it for legs, what hangs tall at its sides for
    arms and flat for wings, the largest on or before it for the head, what trails it for the tail, and gives the
    rest to the part nearest; each gets a bone pivoted where it meets the body, named by part and side, with a
    `BoneRole` (kept in the file) for what animates it to go by. `add_joint` cuts a box across its longest side and
    hangs the far half from a bone at the cut, each half showing the pixels it showed. Editor: Make Bones and Add
    Joint in the bar, a bone's Role in the Inspector. Tests: a figure on two legs, a beast on four with head and
    tail, a blob, wings, a knee and an elbow; the editor making bones, showing a role, cutting a leg and undoing.
    Manual: Box Models.
  - ~~**Motion from how it moves:**~~ done: `animation::MobMotion` (`mob_motion.rs`, `[obj.mob_motion]`, Add
    Component ▸ Animation ▸ Mob Motion) poses a model each frame from how its Obj moved, with no clips. `MobPoser`
    is the motion alone, from a skeleton, its bones' roles and a `MotionInput`: legs by gait (two in turn, four as
    diagonal pairs, more as two sets) stepping by ground covered, lower halves bending as they come through, arms
    against the legs, the body bobbing, leaning and rolling, the head turned to a point within its limit and not
    at once, a tail sprung by turning, wings lifting, breathing at rest, an attack and a flinch over it all.
    Roles reach it through the model: `PreparedModel`/`CookedModel`/`ImportedModel::roles` (`COOKED_VERSION` 12),
    `Content::bone_roles`, `give_mob_bones`. The Model window's Motion mode shows it on the model, posed on the
    CPU (`BoxModel::mesh_posed`), standing, walking or running. Tests: gaits, knees, steps matching ground
    covered, look limit and lag, tail, attack and flinch; the engine walking a model from its file and from built
    content; the editor showing it walk and putting it back. Manual: Mob Motion. Still to come with the block
    world component: its C# side.
  - ~~**A raised second layer:**~~ done: `ModelBox::layer` (`BoxLayer { skin, height }`, `[bones.boxes.layer]` in
    the file), a second place on the sheet whose solid pixels are built standing out from the box's faces
    (`box_model/layer.rs`): on the box grown by the layer's height, so faces' layers meet at its edges; a row of
    pixels sharing one top; walls only where a solid pixel has nothing solid beside it, and none under. The mesh
    carries each layer flat as well, as a second LOD whose error is the layer's height, so far off it is drawn as
    a picture. Laying out, mirroring and cutting a joint take layers with them; `layer_pixel` gives the layer's
    pixel over a face's. Editor: Layer and Height on a box in the Inspector, Layer in the paint bar sending
    strokes to the layer of the box under the pointer, layers' places outlined on the sheet. Tests: a block's
    bounds, winding and pixels, shared tops and walls, a flat layer, the far level, the file; room for a layer
    and its pixels over the box's; the editor adding a layer, painting it raised, changing its height and
    undoing. Manual: Box Models.
  - ~~**In a block world:**~~ done: `block_mob::BlockMob` (`[obj.block_mob]`, Add Component ▸ Terrain ▸ Block Mob,
    which adds the Character Controller it walks with) walks its Obj through a world of blocks: to a point, after
    an Obj, or wandering near where it began (`MobGoal`). Ways are found on the blocks
    (`voxear_terrain::blocks::path::find_path`: a block at a time, up one, down three, as many clear over the
    ground as it is tall, water waded a block deep and gone round deeper, a search that gives up after so many
    places), found afresh every three quarters of a second, and walked with `PhysicsScene::move_character`, its
    step a block high; it turns to face its way, falls, and swims up in deep water. It runs before animation, so
    a Mob Motion on the same Obj walks the model. C#: `Obj.BlockMob`/`AddBlockMob` (`MoveTo`, `Follow`, `Wander`,
    `Stop`, `Arrived`, `Goal`, `Speed`, `TurnSpeed`, `Swims`) and `Obj.MobMotion`/`AddMobMotion` (`LookAt`,
    `LookAhead`, `Attack`, `Hurt`, `MoveAs`, its amounts); native API 77. Tests: ways over a floor, round a wall,
    up a step, down a ledge, through a ford; a mob standing on a level's blocks, walking to a point with its legs
    swinging, climbing a block, going round a wall, wandering within its reach; a script adding and telling
    both. Manual: Block Mobs, Mob Motion.
  - ~~**Clips keyed on a timeline:**~~ done: a box model carries clips (`BoxClip`, `[[clips]]` in the file):
    for each bone that moves, keys saying how far it is turned, moved and sized from rest at a moment, with how
    it goes on to the next (`Ease`: linear, smooth through the keys either side, step). A looping clip goes from
    its last key round to its first. `BoxModel::baked` makes every frame of one, and `prepared` gives the
    model's clips as a model file's own, so an Animator plays one by its name and builds cook them. While an
    Obj's Animator has a clip, its Mob Motion waits. The Model window's **Animate** mode shows a timeline under
    the view (`model_timeline`): clips made, named, sized and looped, the moment shown, a row of keys a bone,
    keys dragged along their rows, Play. The bone picked has rings to turn it (`Handle::Turn`), and Key rows in
    the Inspector; either keys it at the moment shown. Tests: easing, loops, keys on steps, the file, baking;
    a clip played by an Animator from the file and from built content with Mob Motion waiting; a clip made,
    keyed by a ring and by a typed number, a key dragged, played, undone, in the editor. Manual: Box Models,
    Mob Motion. Not done: timeline zoom, copying keys, moving several at once.
  - ~~**Boxes sculpted from cubes:**~~ done: a box can be made of small cubes, one to each pixel of its size,
    each a colour of the model's palette or left out (`ModelBox::voxels`, `BoxModel::palette`; `voxels` and
    `palette` in the file), for rounded and stepped shapes: hair, hands, leaves, a tree's crown. Only the faces
    that show are built, those of one colour side by side as one, into the model's one mesh, coloured from the
    palette's pixels under the sheet in its texture, so bones, Mob Motion, clips and cooking are unchanged. A
    ray meets the first cube in its way (`BoxHit::cell`). `to_voxels` makes a plain box one of cubes that looks
    the same; `set_voxel` grows the box to hold a cube put past its edge. The Model window's **Sculpt** mode
    adds, removes, paints and picks cubes, a stroke working one layer of the model as it found it, with
    Symmetry; the Inspector's Voxels toggle turns a box either way. Plants and trees are box models too: rays
    for dropping and for painting a Foliage's plants meet a Block World's ground (`scene_tools::mesh_hits`).
    Tests: cubes from a box's pixels, faces built and merged, growing, the ray, the file, mirroring; a head
    sculpted, coloured, mirrored and undone in the editor; the ground of a Block World found from above.
    Manual: Box Models, Painting and Cutting Plants. Not done: `.vox` files brought in as boxes of cubes,
    tools that fill or carve many cubes at once, characters made from parts picked at random, plants and trees
    placed by a Block World's generator.

## Gameplay kits: game-ready systems

Systems a game needs again and again, ready to add to an Obj and play, so a new project moves in minutes rather than
weeks. Each is a component with sensible defaults as the easy way in, and every part of it open as the advanced
one: settings to tune in the Inspector, events to hook, and the code itself to read and change, since a kit is
plain C# in a package (see Packages). They build on input and physics (M4) and, for characters, animation (M6).

- **Gun kit (in progress).** Built so far: guns held, aimed, fired, cycled and reloaded in a character's own hands,
  worked out as it happens from what the gun is; recoil through the First-Person View; guns held by arms clips; a
  Gun Handling asset and its Inspector; and guns over the network. They are moving out of the engine into a
  package of their own. Still to come:
  - hitscan and projectile weapons, fire modes (single, burst, automatic), ammo, magazines and reloads
  - procedural recoil: kick and climb from curves, recovery, and camera and weapon sway, so guns feel weighty
    with no animation authored
  - spread that grows with firing and moving and settles when still, aim down sights, and bloom
  - impacts by surface (decals, particles, sounds), muzzle flash and shell ejection
  - damage with falloff and hit zones, and an interface for anything that can be hit
- **Heat and cold ✅.** Every place has a temperature (`voxear_engine::heat`): the climate, from a world's
  generator or the level's **Climate**, warmer in the afternoon and colder before dawn and swinging wider where
  it is dry; drawn to the deep ground's mild temperature under blocks, or the water's in water; then **Heat
  Zones** (a box or sphere that sets or adds, with a soft edge) and the hottest and coldest of what is near:
  **Heat Sources**, blocks with a **Heat** (`BlockDef::heat`, a °C and a reach, each chunk's hot and cold blocks
  listed once until it changes), lava Pools and Streams, and heat scripts set off for a while. Sums and products
  only, the same on every machine. The level's Climate holds a clock: time of day, day length, and whether it
  turns the first Directional Light. A **Heat Body** follows its place's temperature on the fixed steps (Warms
  In); its **Changes** each count while it is above or below a °C and hold still while not, and once one has
  counted long enough scripts hear of it by name and the Obj becomes the change's prefab, starting as warm. A
  character **Feels** Freezing, Cold, Comfortable, Hot or Scorching by its Comfort widened by Cold and Heat
  Resistance, and `Beyond` gives how far past its band it is; what that does is the game's. An item's
  **Dropped** prefab is what it is when it lies in the world, so dropped food cooks. Scripts: `Heat.At`,
  `Heat.Sample` (`HeatSample`), `Heat.AddSource`, `Clock`, `Obj.HeatBody` (`Temperature`, `Feels`,
  `ColdResistance`, `HeatResistance`, `Progress`), `HeatBody.Changed`, `HeatBody.FeelsChanged`, `HeatSource`
  and `HeatZone`; native API 81. Editor: the four components in Add Component ▸ Heat, a block's Heat, and the
  Scene view's **Heat** shading, the ground round the camera tinted by temperature. Tests: a generated desert
  by day and night, magma blocks, a cave in a hot land and a cold one, raw meat seared on a hot stone and frozen
  in the snow, feelings by resistance. Manual: Heat. Not done: fire that spreads; what heat does to the weather;
  a GPU tint over every surface in place of the Heat shading's squares.
- **First-person controller:** walk, run, crouch, jump and slide, slopes and steps, head bob and camera sway, and
  the hands and camera the gun kit mounts on.
- **Third-person controller:** an orbit camera that keeps out of walls, movement relative to the camera, lock-on,
  and root-motion or in-place animation through the animation system.
- **Top-down controller:** for twin-stick, isometric and click-to-move games, with aim toward the pointer or the
  stick, and a camera that leads where the player is heading.
- **Vehicle controller:** cars, trucks and motorbikes on the vehicle physics (M4), with a chase camera, input
  for keyboard and gamepad, and presets to drive at once.
- **Destruction kit,** for buildings and props that break apart:
  - fracturing in the editor: a mesh split into pieces (Voronoi, slices, or glass shards), its inside faces given
    a material of their own, and the pieces kept with the model so breaking costs nothing to work out at runtime
  - structural integrity: the pieces of a building joined in a graph of what holds up what, so knocking out a
    wall brings down what rested on it, floor by floor, rather than everything at once or nothing
  - damage from the gun kit's hits, explosions and impacts, with a strength for each material (wood, brick,
    concrete, glass) and pieces that crack before they fall
  - debris that settles, sleeps and fades, pooled and capped so a collapse never stalls the frame
  - dust, sparks and sounds by material, and a whole building drawn as one mesh until it first breaks
- **For all of them:** input actions bound by default for keyboard, mouse and gamepad; a sample scene each to try
  it in; a VoxDocs page each; and saving and loading their state with the game.

## M7 — Materials and particles (in progress)

- **Custom shaders ✅.** A `.shader` asset (`AssetKind::Shader`) in Slang: a struct of `[Property]`
  fields (numbers, vectors, `[Color]` colours, toggles, `Picture`s), each with a default, and any of:
  - `surface(props, input, inout output)`: colour, normal, metallic, roughness, emission, alpha (clip);
  - `vertex(props, input)`: an offset for each vertex, with `[MaxOffset]` widening culling bounds;
  - `light(props, light, surface)`: the light a surface takes from the sun and each lamp, in place of the
    standard BRDF;
  - `[Unlit]` on the struct: only the surface's colour and emission, as they are.

  A material picks its **Shader** (Standard, or a project shader) and keeps its properties in the `.material` file;
  the material Inspector shows them as fields. On the GPU a material has a shader number and an offset into a
  buffer of property `float4`s; the project's shaders are compiled into the main shaders as one generated `switch`
  on the shader number (`project_shaders.slang`), so the GPU-driven culling and draw lists are unchanged and only
  materials using a custom shader pay for it. The editor compiles with the Slang compiler shipped beside it
  (`SLANGC`, then `slang/bin/` next to the executable with its libraries in `slang/lib/`, then `PATH`), on a thread,
  recompiling when a shader is saved, against the engine's shader sources shipped in `bin/shaders/`. A game build
  ships every shader asset and compiles the modules that draw materials into its content's `Shaders/` folder,
  again only when a shader or the engine changed; the player draws with those, needing no compiler. The shader
  Inspector shows its properties, functions, and compile state or error; the material Inspector edits its
  properties; New Shader and New Unlit Shader start from templates; the manual's Custom Shaders page.
- **Built-in shaders ✅.** Toon, Storybook, Triplanar, Water, Hologram, Sway and Lava, embedded in the engine and
  picked as `builtin:toon` (`BuiltinShader`, `ShaderRef`), numbered before a project's own and compiled in only
  once a material uses one. New Shader… offers Lit, Unlit or a copy of a built-in one. Lava's colour is blackbody
  colour. For them the shader API gained `SurfaceInput.thickness` and `glass` (the glass pass reads the
  depth and tests itself against it), and `SurfaceOutput.reflectance`, `transmission` and `data`; the glass pass
  runs a custom shader's surface and light functions; bounced light is read where a moved vertex rests; and a
  material that moves its vertices always draws its full mesh, so its shadow matches it.
- Shader Graph with previews and sub-graphs, plus material variants, writing custom shaders: on the graph compiler
  particles share, whose shaders must cost what hand-written ones do (see
  [ARCHITECTURE.md](ARCHITECTURE.md#materials-and-shader-graph)).
- **Particle Systems, the stateless path ✅.** A `ParticleSystem` component (`voxear_render::particles`): a round
  with a stream and a burst, looping or once; each particle's lifetime, speed and size ranges, its end size, colours
  and opacity as it starts and dies, intensity, gravity, drag and spin; Point, Sphere, Cone or Box shapes; Alpha or
  Additive blending, lit or glowing, soft before what they meet; a seed. Every particle is worked out where it is
  drawn (`shaders/particles.slang`) from its slot and the system's time, its slot coming round once its last
  particle died, so nothing is kept between frames and every machine draws the same; drawn after glass, the farthest
  systems first. The engine starts each as it appears (`ParticlePlayback`); scenes, prefabs, the Inspector, copy and
  paste, undo, the Hierarchy's + menu and Add Component ▸ Effects; C# `Obj.Particles`, `AddParticles`, `Play`,
  `Stop`; the manual's Effects ▸ Particle Systems.
- **Particle fog and curves ✅.** Particles are drawn after the fog, each fogged and hazed as far off as it is.
  `voxear_core::curve` has a `Curve` and a `Gradient` of up to eight keys, kept in place so they copy as plain
  values; a Particle System's size, opacity and speed over life and colour over life use them, the shaders reading
  their keys; the Inspector edits them on a graph and a strip in their rows (`curve_fields`), the field Vehicles'
  tyre curves were waiting for. Still to come: curves from C#.
- **Smoke, stage 1 ✅.** A `Smoke` component fills a grid of 32³ cells out from its Obj, nearest first in half
  cells across the 26 round each, as much as a ball of its size holds, against the colliders that stay put and past
  none in the way (`voxear_engine::smoke`; in the editor, colliders made for the fill), noting how far out each cell
  is so it spreads from its middle. The renderer marches the smokes at half size (`shaders/smoke.slang`): cells blended
  and read through drifting noise so its edge billows, the sun through the cloud and its shadow map, the light round
  it, fogged as far off as it starts; then lays them over the scene, each pixel from the half-size ones nearest it in
  depth. Lasts and thins away.
- **Smoke, holes ✅.** Each smoke keeps its last 32 shots and 5 blasts (`SmokeHoles`): a shot opens a tunnel half a
  metre wide along its line, filling back in over a second and a half; a blast clears a ball, all but 2%, filling
  back in over five seconds. The shader carves them out, their edges ragged by its swirls; `voxear_engine::smoke`
  makes them where a line or a blast meets a smoke's grid, and `see_through` adds up how thick the smoke is along a
  line, holes and all. C# `Smokes.Shoot`, `Blast` and `SeeThrough`.
- **Smoke, polish ✅.** Smoothed over frames: each half-size march blended with last frame's, found where its light
  came from then and kept within the colours round it; the sun's shadow looked up a little off each step, differently
  each frame, so shadows in it soften; lamps light it; swirls through and through, thinner by the eye, so it reads
  from inside; a fresh blast pushes it into a thicker shell. At the end of its life it disperses: breaking up from its thin
  swirls into wisps, rising and drifting apart in broad slow swirls, thinning to nothing. Still to come: drifting in the wind, filling again as
  walls change.
- **Simulated particles ✅.** A Particle System's **Space** (Local or World), **Wind** and **Turbulence** (with its
  size): any of them makes it a simulated system, its particles kept on the GPU and stepped each frame in one
  compute pass for every system and view (`voxear_render::particle_sim`, `shaders/particle_sim.slang`), and the
  Inspector's Simulated row says which. All share one pool, 36 bytes a particle, a run of it each, filled in a
  ring, so nothing is counted or sorted and they draw in the order sent out; only systems with particles out are
  stepped. A new particle starts where its age has it, so a system first seen running starts full. The wind is
  the Wind Zone's, gusts and all, as plants lean by it; turbulence is a curl of noise. **Project Settings ▸
  Quality ▸ Budget** (`[quality] particles`) is the most particles all may have room for, a million by default;
  the stats overlay, the Profiler and `vox render` count them. On an RTX 3070 a million are stepped in 0.19 ms
  and drawn in 0.9 ms. `samples/Effects` has a campfire, sparks, dust and a million sparks in a wind. C#
  `Space`, `Wind`, `Turbulence`, `TurbulenceSize`, `IsSimulated`. Tests: emission and the pool's room without a
  GPU; on one, a simulated system against one worked out from age, wind either way, turbulence, particles left
  behind, the budget, and a million timed.
- **Particle collision and forces ✅.** **Collision** set to Depth lands a system's particles on what the main
  view sees, by its depth: put back on the surface they have come to, whose facing is found from the pixels
  beside it, and sent off it by their **Bounce** and **Friction**, losing **Life Lost** of what life they have
  left; what is out of view stops nothing. The simulated particles' step moved in among the main view's passes,
  after its depth is drawn, for this. A **Force Field** component (`voxear_render::forces`: Attract, Repel,
  Vortex or Blow, a strength and a radius) moves the particles of systems whose **Forces** take it, as do
  blasts (`Particles.Push` from C#, `voxear_engine::particles::blast`), over in 0.3 s, and the pushers that part
  plants, which drive particles out of their way; 16 fields and blasts a frame. Scenes, prefabs, the Inspector,
  copy and paste, undo, the Hierarchy's + menu, Add Component ▸ Effects, and the field's reach drawn in the
  Scene view; C# `Obj.ForceField`, `AddForceField`, `ParticleSystem.Collision`, `Bounce`, `Friction`,
  `LifeLost`, `Forces`. `samples/Effects` has sparks bouncing down steps and a whirl in its dust. The manual's
  Force Fields. Tests: fields and blasts gathered without a GPU; on one, sparks landing, dying and bouncing on
  a floor, a field pulling either way, and a blast.
- **Particle effects, textures and starters ✅.** A Particle System's **Effect** plays an effect in place of its
  own settings: several emitters, each a Particle System's settings with a name, played together, each a system
  of its own to the renderer and the simulation (`voxear_render::effects`). Eleven starters are built in (Fire,
  Smoke, Sparks, Dust, Rain, Snow, Blood, Muzzle Flash, Explosion, Magic, Mud), and a project's own are
  `.particles` assets (`AssetKind::Particles`): **New Particle Effect…** copies a starter, the file is an
  `[[emitter]]` table each, read by `Content::particle_effect` and taken up again when it changes, and its
  Inspector lists its emitters. **Texture** draws particles with a built-in texture (Puff, Flame, Ring, Flare,
  Shard, made on the CPU when the renderer starts, most of them four shapes to a picture) or a project's
  picture with its **Tiles**, one picture a particle or, with **Animate**, each in turn over its life;
  **Stretch** draws them along their way; **Delay** holds an emitter back. The engine lists the effects and
  pictures the world's systems name for the renderer each frame (`particles::resolve`). An effect that plays
  once plays again while selected in the editor. `samples/Effects` has a row of starters. C# `Stretch`,
  `Delay`. Tests: references, the built-in textures, an effect's file, and each starter drawn on a GPU.
- **Editing effects, and playback ✅.** A `.particles` asset opens on its own as a prefab does, in place of the
  level: an Obj an emitter, each with a Particle System (`particles::effect_scene`), so the Hierarchy adds,
  removes, renames and orders emitters and the Inspector edits them, with undo and copy and paste as anywhere.
  Save writes the Objs' Particle Systems back as the effect (`scene_effect`, in `Levels::save`), and systems
  playing it take it up. The bar over the Scene view names the effect, with Back and Close Effect. A playback
  strip over the Scene view, for a selected Particle System or every emitter of an open effect: pause, Restart,
  a speed (1×, 0.5×, 0.25×, 0.1×, 2×) and a scrubber; an effect that ends plays again. Particles run on a time
  of their own for it (`ParticleTime`, `ParticlePreview`), the engine's but for the preview. Tests: the
  particles' time, an effect to a scene and back, and in the editor an effect opened, played, edited and saved.
- **Lit smoke and lights from effects ✅.** A lit particle with two maps is lit from six sides: the light from
  its right, left, top, bottom, front and back is worked out where its quad is placed, the sun's through its
  shadow and each lamp's from its cluster, and each pixel takes its share of each from the maps
  (`particleLightSix`). The built-in Puff has maps of its own, made with it as a ball of smoke, so every lit
  Puff is; a project's picture names its second as its **Light Map**. Animated pictures fade one into the
  next. A system's **Light** casts a point light in its colour, a stream's flickering and a burst's flashing
  and fading as its particles die, to its **Light Range**; the 16 nearest shine (`particles::lights`, added
  beside the lamps). The Fire, Explosion, Muzzle Flash and Magic starters cast one. C# `Light`,
  `LightRange`. Tests: the maps, a flicker and a flash, and on a GPU a puff lit from either side by a lamp and
  a floor lit by a system.
- **Trails and mesh particles ✅.** A Particle System's **Trails**: **Particles** draws each a tail along the way
  it has come, a strip through the last 15 points of it, which the step keeps after the system's particles in
  the pool, one a tick of the trail's clock, so a tail is as long in seconds as **Trail Time** says whatever the
  frame rate, bends round a bounce and curls in turbulence; **Ribbon** joins the particles in one strip in the
  order they were sent out, which the ring already has them in, reaching on to the Obj while it sends them.
  Both narrow or fade with their particles, are drawn with the particles' own pixels, and never thinner than a
  couple of pixels, fading instead. **Mesh** draws each particle as a mesh that tumbles as its Spin says, as wide
  as its Size across: a built-in Cube, Ball, Chunk or Rod (`voxear_render::particle_meshes`) or a model's first
  mesh with its own material under the system's colour, lit as a surface is, in a pass of its own that draws
  into the depth, so meshes hide one another and the flat particles behind them, and writes its own motion for
  anti-aliasing and motion blur; a simulated one tumbles as fast as it is going, so it lies still once landed.
  The Sparks, Explosion, Fire
  and Magic starters' sparks, embers and motes have tails, and the Explosion's debris and the Mud's clods are
  Chunks. `samples/Effects` has a ribbon, rubble and shell cases. C# `Trails`, `TrailTime`, `TrailWidth`. Tests:
  the meshes, a tail's room and its values without a GPU; on one, tails, a ribbon, and meshes lit, glowing,
  hiding sparks and lying still once landed.
- **Half-size particles ✅.** A system of soft flat particles wider than 0.06 of its distance from the camera is
  drawn into a half-size target, each pixel tested against the scene's depth, and laid over the scene from the
  four half-size pixels round each pixel, weighted to those tested against a depth like its own, so nothing
  bleeds over what stands in front. Systems further off than any of those are drawn before, the rest after.
  6,000 overlapping puffs draw in 0.70 ms against 2.26 ms at full size (640×360, RTX 3070). Tests: on a GPU, a
  column of smoke the same either way, a post in front of it kept clean, and the cost.
- **Playing effects from scripts ✅.** `Particles.Play(effect, position)` plays an effect at a place, upright,
  with its up along a direction, turned, or on an Obj to go where it goes; `voxear_engine::particles::play`
  gives it an Obj of its own (`PlayedEffect`) that the engine takes away once the last of its particles has
  died, or for one that loops, once stopped. Each play has a seed of its own, mixed into every emitter's
  (`particles::seed_of`), so two explosions differ; the same seed gives the same particles, for a networked
  game. C# `ParticleEffect` is an asset type with the starters as `ParticleEffect.Explosion` and the rest, a
  field of it picked in the Inspector from the starters and the project's effects, and `ParticleSystem.Effect`
  reads and sets a system's. `samples/Effects` plays its explosions, muzzle flashes and mud from a script.
  Tests: plays, seeds and clearing up without a GPU, and the C# API without the engine.
- **Particle hits and curves from scripts ✅.** A Particle System's **Hit Events** (with Collision on) has the
  step write each strike, where, the surface's facing and how hard, the first 512 a frame among every system,
  copied back with the counters and read once that frame is done (`particle_sim::ParticleHit`,
  `Renderer::take_particle_hits`); the engine tells scripts before LateUpdate, `OnParticleHit` on the behaviours
  of the system's Obj and `Particles.Hit` for every listener (managed callback `particle_hits`, API version
  78). C# `Curve` and `Gradient` types, and `ParticleSystem.SizeCurve`, `OpacityCurve`, `SpeedCurve`,
  `ColorGradient` and `HitEvents`. Tests: on a GPU, strikes on a floor from the system that tells and none from
  one that does not; curves in words and hits told only of systems still there; the C# types.
- Particles, still to come, in this order (the design is in [ARCHITECTURE.md](ARCHITECTURE.md#particles)):
  - collision with what is out of view: the ground and colliders, and the scene's rays
  - sub-emitters and events; values an effect shows on the Particle System that plays it
  - sorting, particles in the fog volume, flipbooks blended by motion vectors, and anti-aliasing told about
    fast flat particles; a picture along a ribbon; a mesh's LODs and shadows for
    mesh particles
  - bursts sent out at a place from a system already playing, and values an effect shows that a play sets
  - the graph compiler shared with Shader Graph, graph mode, custom Slang modules, and each node's cost shown
  - sleeping off screen, level of detail by distance, and the particle debugger
  - decals on hit, heat distortion, shadows cast by particles, blending in any order

## M8 — Terrain (in progress)

Heightmap terrain first, then digging, generation, block worlds and smooth voxels: each its own
component, sharing generation, surfaces and plants where they fit. Gameplay data (heights, voxels, collision, saves)
is worked out on the CPU, the same on every machine; drawing, culling and erosion run on the GPU. The design is in
[ARCHITECTURE.md](ARCHITECTURE.md#terrain).

- ~~**Terrain you can see and walk on:**~~ done: a `voxear_terrain` crate (heights in tiles of 256 quads that share
  their edges, held as `Arc`s so a change to one leaves the rest; the ground between heights two triangles a quad,
  split as Jolt splits height fields; rays over it; CDLOD patch picking; the `.terrain` file and its `.heights`
  beside it, byte planes then LZ4). A Terrain module and component: heights read from the asset with or without a
  renderer, flat where missing, made from a 16-bit heightmap picture where the file names one. Drawn by the GPU
  from a buffer of tile slots that only changed tiles are copied into (**Terrain uploads**), patches picked on the
  CPU for the camera and each sun cascade and drawn from one indexed grid, each vertex sliding onto the next level's
  grid (geomorph), shaded by the forward passes' own fragment shaders: early, so hills hide what is behind them,
  into the cascades and the depth passes; a coarse mesh a tile for the GI's rays and the sea floor, which only rays
  see (`INSTANCE_RAYS_ONLY`). Jolt height fields a tile, made again for a changed tile and moved with the Obj; rays
  and collision events give the terrain's Obj. Editor: Add Component makes one on its own asset; the Inspector sets
  Data, Size, Resolution, Heightmap and Height, Material, Texture size, Detail distance and Cast shadows, each
  undoable, kept as the asset's in memory and written when the level is saved; New Terrain in the Project panel;
  the Scene view picks and places on the ground. C#: `Terrain.HeightAt`, `NormalAt`, `TryGetGround`, `Raycast`,
  `Exists` (native API 60). Tests: crate, GPU (hills with no cracks between levels, a ridge's shadow), physics, scene
  files, the editor and scripts. Manual: Terrain.
- ~~**Layers, stamps and sculpting:**~~ done: the `.terrain` file's layers as the terrain's source (base height,
  then Import, Stamp, Noise, Sculpt, Spline, Terrace and Smooth, each with a blend of Add, Subtract, Max, Min, Set or
  Multiply, an opacity, height and slope masks with a feather, and sizes in metres; a Phase 1 heightmap read as an
  Import layer). Composed on the CPU, tile by tile on worker threads with a border of 16 samples for what looks at
  its neighbours, over the tiles a change reaches; while a layer is edited, the ground below it is kept, so a brush
  or a dragged stamp makes only that layer and those above it again (release: 4097² from four layers in 0.3 s, a
  brush-sized change in 14 ms). Noise from integer hashing, the same on every machine. Sculpt layers' rasters are
  shared tiles in a `.layers` file, left out of builds; `.heights` keeps a hash of the layers it was made from, and
  heights made from other layers are made again on load. Editor: Layers, Sculpt and Settings along the top of the
  Terrain section, as Unity has them; the layer list (top first, picked, switched, dragged, removed, Add Layer by
  kind) with the picked layer's settings under it; brushes Raise or Lower (Shift), Smooth, Flatten, Set Height
  (Ctrl+click takes the height), Terrace and Noise, painting in the Scene view under a ring on the ground, `[` `]`
  for size and with Shift strength; a picked stamp's outline and its dots to move, turn and size it, a spline's
  points to move; each stroke or drag one step to undo. The GI's meshes for rays wait until a tile stops changing.
  Tests: compositing, masks, brushes, rasters and files; GPU (a changed tile is drawn changed); the editor (layers
  from the menu, a stamp's dot dragged, a stroke in the Scene view, each undone); layers made on load. Manual:
  Layers, Sculpting. Still to come: pictures dropped onto the list as stamps, mesh stamps, a stamp brush, and the
  ceiling warning.
- ~~**Surfaces, painting and holes:**~~ done: up to 32 surfaces in the `.terrain` file, each a material (picked from
  the project's, read through the bindless table, so no texture arrays), its tile size, how sharply it meets the
  others by its height picture, and triplanar for cliffs. Surfaces stack from the ground's own up, each covering
  those below Everywhere, where Painted, or by Height and Slope (the layers' masks, with edges that wander by a
  noise of their own), what was painted added to or taken from that, so rules follow the ground as it changes. Each
  sample keeps its four largest shares, worked out on worker threads over the tiles a change reaches (4097² with
  three ruled surfaces in 0.3 s, a brush-sized change in 1.4 ms) and kept in `.heights` beside the heights, so a game
  reads them as made. The fragment shaders blend the four samples round each pixel, then by height, through a hook
  the forward and depth passes call for terrain. Holes: a Holes brush under Sculpt (Shift fills), a bit a sample,
  every triangle touching one cut away in drawing, shadows, rays, `TryGetGround` and physics (Jolt's no-collision
  height). Editor: the Paint tool, its surface list (Add Surface opens the material picker), the picked surface's
  settings and the brush; each stroke one step to undo. Tests: shares, painting, holes and files; GPU (32 surfaces
  each where its shares are and blended between; the ground below seen and sunlit through a hole); physics (a ball
  falls through a hole); the editor (surfaces added, painted, a hole cut and filled, each undone); shares read as
  written and worked out again when the surfaces change. Manual: Surfaces, Painting, Holes. Still to come:
  distance resampling, a basemap far off, variants and hex tiling against repeats; curvature and flow rules;
  materials dropped onto the list.
- ~~**Plants on terrain:**~~ done. Placed plants: a plant's place, turn, size and lean are a `Plants` component of
  its own (in `voxear_scene`), so a server with no renderer has them; the Foliage's Paint tool paints its plant
  onto whatever is below the brush, terrain or mesh, to a density, between two sizes, leaning with the ground as
  far as asked, off ground too steep and no nearer than its spacing, Shift to erase, each stroke one step to undo.
  A plant taken away is only marked gone, so nothing is built again and the rest keep their numbers; trunks
  collide as a static capsule each, which a ray names the plant of; a Cut prefab is made where a plant cut down
  stood. C#: `RaycastHit.Plant`, `Plant.CutDown`, `Remove`, `Exists`, `Position`, `Size`, `Plants.Query`; a save of
  the scene keeps which plants are gone (native API 63). Manual: Painting and Cutting Plants. Grass: up to eight
  kinds a terrain in its `.terrain` file, each a mesh and material, a density, sizes and a draw distance, growing
  everywhere, where painted, or on a surface as thickly as it covers, off steep ground; a byte a kind a sample,
  worked out on worker threads over the tiles a change reaches and kept in `.heights` (version 4). Each view
  scatters every kind on the GPU round its camera on a grid fixed to the world (same blade, same place), out of
  holes and out of view, into a run of blades per kind and ring of distance with room for every cell the ring
  reaches, drawn by indirect draws with the GPU picking each ring's level of detail by its mesh's error; no
  instance of the scene's each, so past the 65,536 cap. Sways and bends as Foliage does; no shadows. Editor: the
  Terrain's Grass tool, its list (Add Grass opens the mesh picker), each kind's settings, the brush growing it or
  with Shift digging it up, each stroke one step to undo. Material files gained `cutout`. Tests: densities and
  painting; GPU (half a field of 85,000 blades drawn, half bare); the editor (grass added and dug up, undone); grass
  grown on load. Manual: Grass. Still to come: scatter layers by rule for placed plants, digging grass from scripts,
  a basemap and impostors for far grass, and the grass's own tint between healthy and dry.
- ~~**Changing terrain from scripts:**~~ done: edits (raise, crater with its rim, flatten, paint a surface, cut or
  fill holes, grow or clear grass) as small commands applied straight to the tiles they reach, in nothing but sums,
  products, square roots and smoothsteps, heights to whole 1/1024 m steps, so the same edits make the same ground on
  every machine; physics and drawing follow by pointer, a crater takes away the plants in it. Each edited terrain
  keeps the ground as built and its edits (`TerrainEdits`), saved or sent as whichever is smaller: the edits (about
  20 bytes each) or each changed tile as how it differs from the one built (heights as steps from their
  neighbours' foretelling, shares and grass XORed, byte planes, zstd). C#: `Terrain.Raise`, `Lower`, `Flatten`,
  `Crater`, `Paint`, `CutHole`, `FillHole`, `ClearGrass`, `GrowGrass`, `TerrainEdit` and `Terrain.Apply`,
  `CaptureEdits` and `ApplyEdits`; `MaterialAsset` names a surface by its material, with a picker; a save of the
  scene keeps the edits; RPCs carry `byte[]` (native API 64). Tests: edits on a tile's edge, both forms read back
  exactly, sizes; a script's crater, physics following it, and edits put back from bytes and from a save. Manual:
  Changing Terrain from Scripts. Still to come: choosing the form a tile at a time, sending a player who joins the
  edits on their own, and digging grass by kind.
- ~~**Tracks in snow and mud:**~~ done: a Track Maker component (width, depth) presses its path since last frame
  into a map of the ground round each view's camera (1024 texels a side, 6 cm each, scrolling by holding the world's
  cells modulo its size), where it touches the ground; each texel keeps its deepest press and the time of the latest.
  Surfaces gained Tracks (how far they give) and Fills in (seconds). The ground's pixels march the eye's ray down to
  the tracks' floor, bend the normal by its slope, pack it darker, and where deep show the first surface beneath;
  nothing collides with tracks. The pass is skipped when nothing presses and the map stays put. Editor: Track Maker
  under Add Component ▸ Terrain; a surface's Tracks and Fills in. Tests: map scrolling and makers' paths; GPU (a track
  pressed into snow showing the ground, staying when lifted, forgotten when scrolled past, filling in). Manual:
  Tracks and Snow. Grass lies flat where a Track Maker trod, on any surface, standing up again over its kind's
  Springs back time (0 keeps it flat); blades turn down about their foot and stop swaying (GPU test: trodden blades
  cover more ground from above, then stand again). Still to come: tracks in the shadows' and vertices' depth, and a
  map that keeps tracks further off.
- ~~**Wet ground, puddles, lava and glitter:**~~ done: a surface's Liquid (None, Wet, Puddles, Lava) and Glitter, so
  each goes where the surface covers, by rules, paint or a script's paint. Wet: darker and glossier. Puddles: water
  standing where the ground lies below the lowest of a ring round it (craters and ruts fill, slopes that dip drain),
  ruffled by the Wind Zone's wind, and rivulets down slopes that fade where thinner than a pixel. Lava: the
  surface's pictures as a crust cracked into plates carried down the ground's fall in two cross-faded sets, glowing
  by its heat (lava's functions now shared with lava Pools and Streams, `shaders/lava.slang`). Glitter: grains
  catching the sun, in shadow dark, fading out by 40 m. Tests: GPU (wet darker, water only in the puddled crater,
  lava glowing, sparkles only with Glitter). Manual: Wet Ground, Puddles and Lava. Still to come: lava lighting
  what is round it.
- ~~**Models blending into the ground:**~~ done: a material's Ground blend (`ground_blend` in its file, metres; the
  Inspector's row): the forward shader finds the terrain under the pixel (the view's terrain records, ended by an
  empty one), and up that far from the ground blends to the ground's own surfaces there, all ground and facing as it
  does where they meet, the edge wandering, further up what faces up; over holes and off terrain, nothing. The
  terrain's surface sampling moved into `shaders/terrain_ground.slang` for it. Tests: GPU (a red block's foot takes
  the green ground, its top stays red, red to the ground without). Manual: Blending Models into Terrain.
- ~~**Digging from scripts:**~~ done: dug ground as voxels where a terrain is dug, two a sample (a quarter metre on
  half-metre ground), in chunks of 32 a side and columns of them 16 samples a side; a column's chunks made from its
  heights (`height − y`) on its first dig, its inside samples (and those between dug columns) holes in the heights.
  Shapes (ball, capsule) dig or fill only corners within two voxels of their surface, clamped to that band and set
  to 1/1024 m steps; ground a dig opens takes its surface. Marching tetrahedra (six a voxel along its diagonal, so
  neighbours share faces) meet the heights on a column's outside faces exactly, with no crack. Drawn from a pool of
  vertices a terrain (chunks made again when they change, by pointer), by the terrain's own pipelines, triplanar,
  the dug surface blended over what was opened; grass grows over dug columns' holes; a mesh body a chunk; rays,
  `HeightAt` and physics find the floor. Edits Dig, Fill, Tunnel and Explode (a bowl with its lip thrown up), saved
  as commands or chunks (edits version 2). C#: `Terrain.Dig`, `Fill`, `Tunnel`, `Explode` (native API 66). Tests:
  no cracks or gaps along a column, rays and floors, save forms, GPU (a pit drawn whole, grass round it), physics
  (a ball in a pit, a character along a trench), a script's explosion. Manual: Digging.
- ~~**Digging in the editor:**~~ done: the Sculpt tool's Dig and Fill brushes (a ball a dab where the ground is
  now, following the pointer over the level the stroke began at, Strength how deep, Bares the surface laid) into a
  Dig layer of strokes; Spline layers with Dig on dug as a trench (Add Layer ▸ Trench): a cross-section of floor,
  leaning walls, a fire step on the front and a parapet heaped along it, swept along the line a couple of metres at
  a time over the ground. Digging layers lay no heights (left out of compositing and its hashes) and are dug after,
  in order: as a terrain loads, and in the editor again only when they or the heights under their columns change.
  `.heights` keeps none of the holes digging cuts. A spade icon. Tests: trench profile, strokes read back, holes
  left out of saves; the editor (a dig, a trench, each undone); a trench dug as a terrain loads. Manual: Digging,
  Sculpting, Layers.
- ~~**Dug ground finished:**~~ done: Smooth begun on dug ground evens it out into the Dig layer (each corner near the
  surface towards the six round it, a few passes a dab, never on a face shared with ground not dug). Dug chunks keep
  their triangles once made, shared by drawing and physics, made on background threads of the terrain's own
  (`voxear_terrain::workers`); drawing and physics keep a chunk as it was until its new triangles are done, and a
  tile holed for newly dug columns until theirs are, so nothing shows or falls through; a terrain loading or first
  seen makes all of them at once. Digging itself shares its chunks over every core (a 4 m crater: 36 ms to 5 ms).
  Each chunk is made at three levels of detail, its points gathered into cells of two and four voxels with those on
  its faces kept, so levels meet with no gap; a view draws the chunks it sees, each at the level the ground round it
  is, runs side by side in the pool as one draw. The tiles' meshes for rays leave out dug columns and carry their
  chunks at their coarsest, so bounced light sees what was dug. Tests: smoothing a trench's lip and leaving the face
  with ground not dug, background triangles and tiles waiting for them, coarser levels with the same edges on their
  faces, a view's picks, rays' meshes over a pit, GPU (the pit whole from far off), the editor (Smooth on dug
  ground, undone). Manual: Digging, Sculpting.
- ~~**World generators:**~~ done: `.worldgen` assets (`voxear_terrain::generate`): a seed, Scale, Sea level and
  Height, three fields read through one warp (Continents, Ruggedness, and Ridges as ridged noise whose crests run in
  long branching lines, their Sharpness set), and four curves drawn smooth through their keys without overshoot
  (Coast and Inland over continents, Relief over ruggedness, Peaks from valleys to crests): ground = Coast + Inland ×
  Relief × Peaks, then Crags on high peaks and Bumps everywhere. Presets: Battlefield Plains, Rolling Hills, Alpine,
  Archipelago, Canyons. Only sums, products, square roots and `floor`, so a seed makes the same bits everywhere.
  A Generate layer (Add Layer ▸ Generate, which picks the project's generator or makes one beside the terrain) lays
  it, Set, from an Offset in the generator's world. New ▸ World Generator; its Inspector: the five settings, and
  under Advanced the fields, warp, curves (graphs drawn as the generator reads them), crags and bumps; each change
  saved and the open level's terrains generated from it made again, as are those edited outside the editor. A
  sprout icon. Tests: the same bits for a seed (a hash), smooth curves through their keys, fields filling their
  range, presets found by shape, a layer across a tile's edge, the editor (a Generate layer, a preset picked).
  Manual: Generating Terrain. Chose four curves combined over nested ones: easier to edit, coasts into ranges alike.
- ~~**Gullies in generated ground:**~~ done: a generator's Erosion, Gully spacing and Gully octaves: in each octave,
  stripes laid round jittered points run down the slope the ground has after the octaves before (its slope from
  four nearby heights, each octave's own adding to it), V-shaped on their floors, cut deeper the steeper the ground
  up to about 30°, never raising it, so small gullies run down the walls of large ones and into them; ridges and
  valley floors left; shallower towards the sea and never below it. Polynomials only, still the same bits for a
  seed. Tests: only cutting, gullies across the slope, level ground left, the hash. Manual: Generating Terrain.
- **Generator graphs**, in parts: a world generator as a graph of nodes; a graph editor with a preview on every
  node; more kinds of node and the presets as graphs; biomes as nodes; overhangs, caves and cave rooms as nodes.
  - ~~**The graph and its evaluator:**~~ done: a `.worldgen` holds its seed, scale and sea level and a graph
    (`generate::Graph`: nodes with numbers of their own, wires from outputs to inputs, and the values it exposes as
    rows of the generator's own). Kinds of node a file each under `generate/nodes/`, saying their ports and values
    for the editor to draw: Landmass (all a generator was, in one node, the same bits), Climate, Noise, Constant,
    Math, Curve, Output. Compiled when made (`generate::compile`) from the Output back along the wires into steps
    that fill rows of numbers, shared where two do the same work, in an order the wires alone set; run 512 places
    at a time (`generate::eval`), each place by itself, so a batch gets the bits each point gets alone. A file from
    before graphs reads as a Landmass and a Climate wired to the Output and makes the same ground. A generator is
    never changed, only replaced, with a hash of all that decides its ground: saved with only a node moved it keeps
    its place in `Content` and nothing is made again, and a terrain's kept heights remember their generators'
    hashes. The Generate layer asks for a padded tile at once, block worlds for a chunk column.
    A graph with a loop or no Output still makes ground and says what is wrong (`WorldGen::faults`). Inspector:
    a landmass's settings as before; any other graph, its seed, scale, sea level and exposed rows. Tests: an old
    file's hash, a landmass in a graph against itself, batches against single places, text read
    back as written, the hash following the ground alone, broken graphs, block chunks by hash before and after.
  - ~~**The graph editor:**~~ done: the Generator panel, opened by a double click on a `.worldgen` or Open Graph in
    its Inspector. A canvas that knows nothing of generators (`graph_canvas`, for other graphs later): drawn, as
    the UI cannot scale a subtree, each node a pooled element of shapes with labels at a font size that follows
    the zoom, wires as short lines; the wheel zooms in steps about the pointer, the middle button or Alt moves
    the view (the UI's pointer now hears the middle button), a drag moves the picked nodes on a grid or boxes
    them, a drag from port to port wires them where the owner allows (`Graph::connect` on a copy decides), a wire
    let go on nothing asks for the search, and Delete, Ctrl+D, Ctrl+C and Ctrl+V, F and Space do as they say. The
    search is the object picker, with the kinds of node in their folders. The picked node shows in the Inspector
    as rows made from its kind's values (`world_gen_inspector::ValueRow`, one arm a kind of value), seldom-needed
    ones under Advanced, a value standing in for an input hidden while that input is wired; a context click on a
    row's label exposes it, and the generator's own Inspector is the same rows: Preset while its shape is a preset's,
    Seed, Scale, Sea level, what it exposes, and Open Graph. The graph has its own undo (whole generators kept,
    changes within 800 ms to one value or to where nodes are merged), which Ctrl+Z, Ctrl+Y and Delete reach while
    its panel or the Inspector showing it was the last pressed in. Saved once nothing is held, so the level's
    ground is made again when a drag ends; the previews follow every step, on a thread of their own
    (`generator_previews`): a picture of every node's first output from one texture, and the ground in 3D as a
    block coloured by height, coarse at once and refined to 257 by 257, turned by a drag. The square of the
    world they show is kept in the file (`generate::PreviewArea`), out of the hash. A graph's faults show on the
    nodes at fault. Tests: the canvas without a GPU (zoom, moving, picking, wiring, the search, what is drawn),
    the history and copies, the previews' image and mesh, and in the editor a graph opened, a node added from the
    search, wired, undone and redone, a value exposed, and the terrain following. Manual: Generator Graphs.
  - ~~**The node library and starter graphs:**~~ done: twelve more kinds of node, a file each. Sources: Cells
    (the ground nearest each of a scatter of points: how far the point, how far the cell's border, a number of the
    cell's own) and Falloff. Shape: Terrace, Clamp (its corners rounded), Blend by a mask, Warp, Smooth; Math
    divides too. Masks eased in over a feather as a layer's are: Height Mask, Slope Mask, Curvature (ridges and
    valleys apart), Climate Mask (the eight biomes' ranges as presets). Gullies as a node on any height, by a
    landmass's maths (`landmass::Gullied`, which both use), each octave joining the floors of the one before with
    no step. A node that reads round a place (a slope, a curvature, an average, gullies) makes rows of places and
    asks for its input there. Angles are rows in degrees kept as rise over run (`ValueKind::Angle`), their feathers turned
    by sums and products, so nothing that makes ground uses trigonometry. Five graphs to start from
    (`generate::starters()`), under the presets' names and tuned by eye against them: Battlefield Plains (streams
    cut into nearly level ground, shell holes from two scatters behind a Craters row), Rolling Hills (smooth hills
    with valleys that branch), Alpine, Archipelago (ragged coasts, shallows, islets) and Canyons (stepped high
    ground, canyons along ridged lines with floors at one level, ravines down their walls), each with its height
    and a few more values as rows. Files made from a landmass preset are untouched. Tests, for every node: its
    range, batches against single places, its text read back; a
    hash of each starter's ground, the same unoptimised and optimised; a tile of each timed against a
    landmass's. Manual: Generator Nodes. In the editor, New ▸ World Generator makes the Rolling Hills starter, and a
    generator's Preset row picks among the starters while its graph is one's still, whatever its rows are set to
    (`Starter::shapes`); a single landmass, as a file from before graphs is, keeps a landmass's presets, and a
    graph of its own has no Preset row, since a preset would take its place. A preset picked with the graph open
    is a step of the graph's to undo. Tests: the row's kinds and a starter picked without a GPU; in the editor a
    new generator's graph, another starter picked and undone, an angle typed in degrees.
  - biomes as nodes: where each is and what it brings (its shape, a Terrain's surfaces, a Block World's blocks),
    laid over one another as layers; surfaces' climate ranges and kinds' covers moved over
  - ~~**Voxels:**~~ done: the Output has a **Volume** beside its height, how far into the ground each place in
    space is. It compiles into two programs (`compile::Voxels`): one run once a column, making what its nodes read
    of the ground (a height, a mask), and one run once a cell, given its middle, its size and those rows carried
    up; a Field is only worked out a cell at a time where a node moved the cells it asks about. The ground under
    the caves is compiled beside the whole, for far levels, for telling open air from a cave's, and for layers.
    Nodes: Solid, Noise 3D, Overhangs (pushes through a mask, steep ground with none wired in, its octaves fading as
    blocks grow too large to hold them), Tunnels, Chambers, Combine (Union, Subtract, Smooth). `WorldGen::volume`
    gives `carried`, `carried_grid` (masks made at every fourth column and read between) and `densities`. Block
    worlds and Voxel Terrains make their ground from it (`blocks::generate`); a generator
    with none has the world's own Overhangs and Caves wired into a copy of its graph as the same nodes, seeded as
    they were, so a world with Overhangs at 0 keeps its blocks to the bit, and **Move to Generator** in the Block
    World's Inspector writes them into the generator with their amounts as its rows. Hillsides are no longer
    pocked: only steep ground is pushed. Layers are counted down each column from the open air, through chunks and
    past caves, so the under block no longer shows through; drawn smooth,
    fills are distances (the number over how fast it changes from block to block), the top block follows slopes as
    a skin and walls are bare, and a corner takes the kind of the ground its surface passes beside. Tests: cells in
    a batch against each alone, from every core; the hash following the Volume; each node; a Field carried and bent; grids agreeing where they overlap; layers on overhanging
    ground of cubes and smooth; the moved numbers making the same ground; the editor moving them. Manual: Block
    Worlds, Voxel Terrain.
  - ~~**Cave rooms:**~~ done: a room is a prefab of **Cave Shape** Objs (Ball, Half Ball or Box; Carve or Fill;
    Chance; Inner Margin) and **Cave Exit** Objs, outlined in the Scene view and built with the usual tools. A
    **Cave Network** node (`generate/nodes/cave_network.rs`, `generate/caves`) lists rooms by prefab and weight and
    keeps each one's shapes and exits as numbers in the generator's file, which the editor reads from the prefab
    when it is picked, saved or changed on disk (`voxear_engine::caves`), so making ground reads no prefab and a
    joining player needs the generator alone. Rooms lie in the regions of a grid, turned by quarter turns and
    mirrored so their exits face what they join, with tunnels from exit to exit: wound lines of sixteen pieces with
    a level floor. **Endless**: a room a region from a hash of the region, a tunnel between neighbours where a
    hash of the pair says so, each room Depth under the ground at its own middle, some with a way up to the
    surface; whatever reaches a region is gathered from it and the two rings round it, so a chunk made alone
    meets its neighbours.
    **Level**: a main path walked over the grid without crossing itself and side rooms off it, laid out when the
    generator is compiled. `WorldGen::caves` gives the rooms as they lie, and scripts read them as `Caves.Rooms`,
    `Caves.Start`, `room.Exits`, `room.SpawnPoint()`, with `Caves.Of(world)` and `RoomsNear` under them. A **Cave
    Level** preset beside the five. Tests: a cell alone against a batch; a chunk column alone
    against one made among its neighbours; hollows whole across region lines; every room of a level reached from
    the first through the blocks; a prefab read, changed and read again, in the engine and in the editor; a
    script finding rooms, exits and floors. Manual: Caves.
  - ~~**Caves made by rules:**~~ done: a Cave Network's list holds rooms made by rules beside prefabs, each kind
    with a weight (`generate/caves/made.rs`, `RoomRules`: Size from and to, Shapes, Height, Roughness, Floor,
    Pillars, Hanging Rock, Ledges, Exits, Exits Apart). A made room is round shapes that overlap (domes, balls,
    squashed balls) run together smoothly into one hollow, sized and placed from the room's own seed; a level
    floor through its lowest third; pillars, hanging rock and ledges put back, none in the way to an exit or
    between two shapes; exits set at its walls. Its walls come in no further than a margin it works out from its
    own shapes, and the noise is coarser the rougher the walls, so roughness closes nothing and opens no hollow
    apart. Tunnels: a width for each between **Tunnel Width** and **Widest**, **Wander**, a **Slope Limit** (one
    too steep is wound wider, so longer and gentler), **Branches** (dead ends off a tunnel) and **Loops**. An
    endless network's regions are taken sixteen at a time (`caves/joins.rs`): the largest group of rooms that
    touch are joined as a tree by numbers from the seed, with some of the joins left over made for loops, and two
    sixteens join through a pair of facing rooms both pick alike. A level takes made rooms and loops too. Places
    on a room's walls, floor and roof (`PlacedRoom::walls`, `floors`, `ceilings`; `room.WallPoints`,
    `FloorPoints`, `CeilingPoints` for scripts), for ore, crystals and plants to come. The Inspector lists a
    generated room's rules under it, and the Generator panel shows one such room by itself, its roof away, while
    its Cave Network is picked. A **Caves** preset: hills over halls and chambers without end. Tests: a made
    room one hollow whatever its seed, its exits open at the roughest; a cell alone against a batch and the
    same bytes from the file; a chunk alone against one among its neighbours; every room of sixteen regions
    reached from any other through the blocks, and the next sixteen through their side; a level of made rooms;
    places on rock. Manual: Caves.
  - ~~**Caves at several depths:**~~ done: an endless Cave Network has **Layers** of rooms, **Layer Depth**
    apart, for tall worlds. A region's middle number is its layer; each layer is joined as one network by its own
    numbers, and ramps join a room to the room beside it one layer down (`Joins::ramps`: one a district of sixteen
    regions, more with Loops), wound longer under the Slope Limit as any tunnel is. What reaches a region is
    gathered for every layer at once. Tests: a ramp in every district, both ends agreeing; rooms at each depth;
    two layers one cave through the blocks; a cell alone against a batch; one layer the bits it was.
  - ~~**Tall worlds, tablelands, arches and raised land:**~~ done: a chunk column leaves out what nothing is in.
    A Volume says how deep its ground may be anything but solid (`Compiler::set_depth`) and between which
    heights ground stands or is hollowed apart from a column's own (`Compiler::note_span`, `Volume::spans`); a
    column asks only the cells between, and whole chunks of rock are one block (`Chunk::tidied`). A column 1024
    blocks tall takes 4 ms plain and 39 ms over three layers of caves. Nodes: **Plateaus** (tablelands at uneven
    heights, walls, rubble, ramps of steps, a river and its canyon), **Arches** (bridges found a region at a
    time across low ground between two high places) and **Raised Land** (islands over the ground, floating or
    on a stalk). Both show in the far view (`Volume::standing`, `far_tile`). Presets **Mesas**, **Sky Islands**
    and **Highlands**. Tests: a tall column the blocks it is asked whole; levels and walls; a ramp walked up;
    an arch's opening and top; a window through a ridge with the ridge whole over it; islands clear of the
    ground; both in far tiles. Not built: shelves hanging off cliffs.
  - ~~**Spires, fins, chasms, roofs and great peaks:**~~ done: **Spires** (towers of rock, tapering, leaning and
    banded, in clusters), **Fins** (thin tilted blades in rows that run alike across the land), **Chasms** (rifts
    and sinkholes cut into the ground wired in, sheer or in ledges, with Floor, Walls, Sunk and Rim beside the
    height), **Roof** (ground laid back over a chasm at its Rim, with holes and pillars, so a chasm becomes a
    cavern under land like any other) and **Peaks** (ranges of sharp ridges with horns over level land). Spires,
    fins and roofs say where they stand a column at a time, as raised land does. The Output's **Sunk** lets a
    biome keep to ground sunk under the land round about (`BiomeDef::sunk`, `WorldGen::sunk`), so a chasm's
    floor and a cavern's grow their own. Presets **Karst**, **Shattered**, **Great Peaks** and **Rifts**. Tests:
    spires solid up their cores and open beside; fins thin and leaning their dip; chasms sheer, sunk as deep as
    cut, kept to a mask; a roof at the land's height with holes open to the floor and pillars to it; peaks with
    level land between and narrower tops the sharper; a chasm's floor taking the sunk biome under a roof taking
    the land's; each preset pinned; GPU renders of all four.
- ~~**Erosion layer:**~~ done: Add Layer ▸ Erosion wears the ground below it on the GPU (`shaders/erosion.slang`,
  `voxear_render::erosion`, `Renderer::erode`), once, in the editor: rain each step, water through pipes to its four
  neighbours sped by the drop and held to what each cell has, its depth and speed, carving where it can carry more
  (by tilt, speed and depth) and dropping where it can carry less, what it carries passed on as the same share of
  its water through each pipe (so none is lost), evaporation, then slumping past an angle worked out to eight
  neighbours and gathered. Grids over 2048 a side worn coarser, the change stretched back. What it changed kept as
  the layer's raster, added like a Sculpt layer's; `made_from` (the ground below and its weather) tells the panel's
  Erode / Erode Again button when to wear it again. Weather: Rain (mm), Steps, Wear, Slump angle. A drop icon.
  Tests: GPU (high ground worn into low, volume kept within 1%, a cliff slumped to 45°), the grid and its change
  sample by sample, the editor (worn as added, again after rain changed, undone). Manual: Erosion.
- ~~**Rivers and lakes:**~~ done: a Rivers layer (`voxear_terrain::rivers`): priority-flood filling from the edges
  and the sea (the topmost generator's below, where it reaches the ground) giving each cell the neighbour it drains
  to and the ground draining through it; rivers where that passes Source (hectares), wider (by the square root of
  the area) and deeper (by its fourth root) downstream; courses traced from each head to the edge, the sea, a lake
  or another river, eased and never rising; beds cut along the courses under the water, rounded floors and banks
  rising a metre a metre; lakes in filled hollows half a metre deep and 2000 m² or more, at their spill height,
  covered by boxes halved until hardly any holds lower ground that drains elsewhere. Kept as the layer's raster with
  its `made_from`; Carve / Carve Again and Add Water (an Obj of Streams on Paths, their water as wide as the banks
  at its height, and Pools) in the panel; Source, Width, Depth rows; new Rivers layers cut as added. Tests: all
  water drains away, filled ground falls along the flow, rivers never rise and run in their beds, a bowl's lake and
  its boxes, nothing cut under the sea, the editor (cut, again with less Source, water added and undone). Manual:
  Rivers and Lakes.
- ~~**Biomes:**~~ done: a generator's Climate (size, temperature and humidity on average and how far they stray,
  cooling with height above the sea; `WorldGen::climate`), with Temperature and Humidity among its first settings
  and the rest under Advanced, presets each with a climate to suit. Surfaces covering by Rules gain temperature and
  humidity ranges beside their height and slope ones (eased over 3 °C and 0.08, wandering with their breakup), set
  at once by a Biome (Snow, Tundra, Taiga, Grassland, Forest, Desert, Savanna, Rainforest; Custom once changed);
  weighing reads the climate from the topmost Generate layer's generator at the terrain's place in its world
  (`ClimateSource`), or from height alone without one; Cover's Rules renamed from Height and Slope. Grass on
  surfaces follows. Tests: climate fields and cooling, snow where cold and desert where hot and dry, the editor (a
  Snow surface nowhere in a mild world, everywhere once its temperatures widen). Manual: Surfaces, Generating
  Terrain. Not done: trees scattered by rules on a heightmap terrain (a Foliage's plants by surface or biome); a
  Block World grows its own (see Block worlds, Plants a world grows).
- ~~**Endless worlds:**~~ done: a terrain's Endless setting (`[endless]` in its file: view distance and spacing,
  0.25 to 4 m) keeps a window of tiles round the camera (`voxear_terrain::endless`), up to 33 a side, its heights
  starting at `Terrain::origin`. Each tile is made alone on the terrain's background threads from the layers that
  make ground without end (Generate, Noise, Stamp, Spline, Terrace, Smooth), with its shares and grass from its
  padded ground, so tiles meet exactly; nearest first, the 3 by 3 round the camera at once, holes until made. A
  change remakes only what it reaches: tiles under moved layers, or just shares and grass. The renderer and physics
  keep each tile in a slot by where it is in the world (`endless::slot_of`, `terrainTileSlot` in the shaders), so
  a moved window uploads and builds only the tiles that came in; physics builds four a step, middle first. The
  engine streams round the game's camera or the editor's Scene view (`terrain::stream`, `TerrainViewer`); no
  `.heights` is kept, builds carry what its layers name. Brushes, Import, Erosion, Rivers, holes, digging and
  script edits stay with terrains of a set size. Tests: tiles meet and match a set-size terrain, slots, the window
  following and keeping tiles, what a change remakes, physics as the window moves, the engine round a camera and
  saving, the editor's Endless setting, the GPU drawing the same ground after a move. Manual: Endless Worlds. Not
  done: coarser far tiles, rivers along valleys without end.
- **More terrain generation:** feeding the layer stack so it can be painted and stamped over afterwards:
  - trees and other plants scattered by surface and biome (done for Block Worlds, which grow their own; heightmap
    terrains to come, with the same rule)
  - stream-power erosion for ranges
  - far tiles of endless worlds made and kept coarser
- **Block worlds**, in parts: blocks you can see and stand on; generation from world generators, with caves;
  editing in the editor and from scripts; far views through levels of chunks; small coloured blocks.
  - ~~**Blocks you can see and stand on:**~~ done: `.blockworld` assets (`voxear_terrain::blocks::file`: block
    size, kinds of block each with a colour and materials for every face, its top and its bottom, and flat
    ground to start as), chunks of 32³ kept sparse with palettes in as few bits as they need (`blocks::chunk`),
    faces merged into quads by surface and corner shading, two words a quad, kept by facing (`blocks::mesh`), and
    a ray stepping block to block (`blocks::raycast`). The renderer keeps a world's quads in a pool of its own and
    draws them six vertices a quad with block pipelines of their own (`blocks.rs`, `shaders/blocks.slang`, the
    terrain's surface shading), each view only the chunks in view and their faces towards it, the cascades every
    face; each chunk's faces merged by surface alone give its collision mesh and its mesh for the GI. Engine:
    `AssetKind::BlockWorld`, `[obj.block_world]`, `blocks::{BlockWorldAsset, make, look, keep, save}`. Editor: Add
    Component ▸ Terrain ▸ Block World on a new `Blocks/<Obj>.blockworld`, its rows (Data, Block size, Ground size,
    Ground height, Cast shadows), undo. Tests: palettes, greedy quads covering exactly the faces a block at a time
    would, corner shading, files, rays, the GPU drawing a field and a pillar shaded at its foot and shadowing, a
    ball resting on blocks, the engine loading and saving, the editor adding one. Manual: Block Worlds.
  - ~~**Editing:**~~ done: `BlockWorld::set_blocks` remakes the chunks a change reaches, and those beside it whose
    faces it meets or shades, on every core; `blocks::edits::BlockEdits` keeps changes from what a world was made
    as, a chunk's blocks one by one while few or whole once smaller, compressed (`VXBLOCKS`); the editor keeps its
    changes from the ground in a `.blocks` file beside the `.blockworld` (builds ship it), and a ground changed
    keeps them. Editor: Build and Settings tools (`block_panel`), the list of kinds (add, move and take out, every
    block renumbered), each kind's Name, Color, Material, Top and Bottom, and the Scene view's brush
    (`block_tools`: Place, Break, Paint, a ball as many blocks across as its Size, Shift turning one into the
    other, a stroke a step to undo). C#: `Blocks.At`, `Raycast`, `Set`, `Fill`, `Ball`, `Break`, `Place`,
    `CaptureEdits` and `ApplyEdits`, saved with the scene (API 68). Tests: chunks remade beside a change, changes
    kept small and read back, a world loading with them, the editor placing, breaking, undoing, adding and taking
    out kinds and saving, scripts asking, building, blasting and putting back.
  - ~~**Generation:**~~ done: a `.blockworld`'s `[generate]` (`blocks::generate::BlockGenerate`: world generator,
    offset, lowest block, overhangs in metres, caves 0 to 1) makes its ground a column at a time from the
    generator's heights, pushed in and out by 3D noise (`noise::gradient3`), tunnels where two noises meet nothing
    and chambers where a third is high, three blocks under the surface kept whole; kinds of block cover it by
    climate and height (`file::BlockCover`, biomes), later kinds over earlier; every chunk column on every core.
    Each world's asset keeps the ground its file made (`BlockWorldAsset::base`, chunks shared with its blocks), so
    changes are kept across a changed ground or generator; worlds are made again when their generator changes.
    Editor: Ground (Flat or Generated) and its rows, the ground's blocks, each kind's Cover rows. Tests: 3D noise,
    columns at their generator's heights, snow where cold, caves hollowing, overhangs, a generated world loading
    with its changes, the editor picking a generator and a kind covering by its biome.
  - ~~**Without end, and far views:**~~ done: generated ground's `[generate.endless]` (view and far distances)
    streams chunk columns round the camera on the background threads, nearest first (`blocks::stream`): near
    columns into the world's own blocks, changes kept for them made as they come (`BlockWorldAsset::pending`) and
    kept as they go; beyond, rings of coarser columns (`generate::column_chunks` at a level, blocks twice as large
    each ring, out to six), each window's sides on its next level's columns, a coarse column left out where its
    four finer ones are made. The renderer keeps chunks by level (`origins.w`) and scales them; physics stands
    things on the near ones alone. Saving keeps every change, near or away (`stream::all_changes`). Editor:
    Endless, View distance, Far distance. Tests: windows, columns coming with their changes and going keeping
    them, coarse columns never over made fine ones, the engine streaming round a camera and saving a far change
    with a near one, the GPU drawing a coarse chunk twice as large, the editor making one round the Scene view.
  - ~~**Small coloured blocks:**~~ done: `.vox` models (`AssetKind::VoxModel`, Block Model; `blocks::vox`: models,
    the palette, and a scene's transforms and groups placing several, z up there and y up here) built by the
    editor's Model brush standing on the face clicked, each colour a kind of its colour, those the world has kept
    and the nearest once its list is full, a click a step to undo with its kinds. Moving a kind of block now
    renumbers every block and kept change even where the ground stays the same. Tests: reading a scene of two
    models, placing into a world's kinds, the editor building one and undoing it, and moving a kind the ground
    has none of.
  - ~~**Cave culling:**~~ done: each chunk's mesh notes which of its six sides see each other through its air
    (`blocks::sight::sights`, flooded at mesh time), and each view walks from the camera's chunk through those
    links, never back along an axis, within the frustum (`sight::seen`), drawing only the chunks it reaches; worlds
    drawn smooth, far rings and shadows are drawn without it. Tests: a wall's sides apart, a closed cave passed
    over and one down a shaft seen, a view drawing only the chunks it can see into.
  - ~~**Water blocks:**~~ done: a kind of block can be water (`BlockKind::water`; `BlockFaces`, each kind's surfaces
    and whether it is water, replace the meshers' `[u8; 3]`). The cube mesher draws the ground's faces where it
    meets air or water, closing no corners in with water, and water's faces only where it meets air, merged by
    surface, after the ground's quads (`ChunkMesh::water`), with no triangles, so physics and the GI's rays pass
    through; chunks see through water (`sight::sights`), and a whole chunk of water buries nothing. The renderer
    draws them with the Pools and Streams (`water::record`, `blocks::draw_water`): a pass finds where each pixel's
    view leaves the water (`blockWaterExitFragment`), then `blockWaterFragment` shades each face as a Pool's still
    water, the kind's colour its Deep, its top rippled, bent and dimmed by as much water as the view goes through,
    foam at the shore, its underside from below. `BlockWorld::water_over` gives Buoyancy the top of the water in
    a column. Generated ground fills the open air below its generator's sea level with its sea block
    (`BlockGenerate::sea`). A Voxel Terrain kept its water as cubes (smooth since, below),
    the smooth ground running under it, each block of water taken as a block further out than the ground beside it, its
    sides drawn against anything not water; balls add ground in place of water and never take it. Editor: each kind's
    Water (its materials' rows hidden), and Sea block under generated ground's settings. Tests: water's faces apart and
    the ground's under it, a chunk of ground beside one of water, sights through water, water over smooth ground, a
    generated sea up to its level, water's top in a column, a ball floating and one sinking to the floor, the GPU
    drawing a pond over its floor, the editor making a kind water, placing it and picking a sea block.
    Flowing (`blocks::flow`): water placed and a generated sea are sources, full for good; water runs from them
    down, then out over ground an eighth of a block lower each block, seven at most, falling as a column off an
    edge, never running out from falling water or over water, and dries a step at a time once nothing feeds it. A flowing
    block keeps its level and its way up in its fill (`flow::Water`), so saves and the network carry it; the mesher
    lowers its top and sides and draws the strip over lower water, each such quad keeping its shape where the
    corners' shading was and its way up in its first word's top bits (`mesh::water_shape`), which `blockWaterVertex`
    moves its corners by. A step looks only at blocks near the last step's changes (`BlockWorld::flowing`, set by
    every change), each from the blocks as they were, so every machine flows the same; `voxear_engine::blocks::flow`
    runs five a second in `Game::tick`, editing and playing, at most four a frame. Tests: levels, reach, falling off
    an edge, drying, a step a block from the blocks as they were,
    quads' shapes, the engine's steps the same however frames fall, the GPU drawing water lower the further it runs,
    the editor's water running out and undone with it. Buoyancy pushes against the pull where a body is, as strongly
    as it pulls, reading how deep it is under the water's top (`BlockWorld::water_depth`).
    After a first look: still water spreads over water (falling water only over ground), so a lake runs into a hole
    dug beside it, and a block between two sources becomes one; a Voxel Terrain's water is smooth, its surface made
    by surface nets over a field of water's and ground's depths, faces only where water meets air, a generated sea's
    top blocks keeping where its surface lies (`flow::surface_fill`, `smooth_surface`), so it lies flat at its sea
    level; block brushes work once a click, again only as the pointer moves half a brush on, and
    while playing; terrains' and block worlds' looks count as using their materials, so their colours survive play
    (they were freed as a level loaded). Tests: a lake running into a hole, a gap filling, smooth water at its top and
    at a sea's level, a block world's colours kept loaded.
    After a second look: drawn smooth, a generated sea takes the ground's top block where the ground lies under it with
    no block of sea over it (`generate::pushed`; the ground below stays, the sea's floor), and its surface runs a block
    into the shore (`smooth::water_surface`: ground beside the water's top with air over it takes the water's level and
    a face), so it meets the ground with no gaps; each smooth corner
    takes the kind of the ground nearest the surface among its eight blocks (`Padded::kind`) and a triangle's three
    kinds blend by how near each corner a fragment is (`blockVertex` packs them in `lod` and `cluster`), which also
    keeps dirt from showing through a thin top layer on slopes. Under water in blocks: the water round the camera is a
    body among the Pools (`water_bodies::block_water`, `WATER_BLOCKS`), level with the top of the camera's column and as
    far as that top runs along x and z, in the kind's colour (`BlockFaces::color`), so the under-water view, the
    waterline and the wet lens are a Pool's; it draws no surface or walls and leaves the sea alone. It is found only
    where a view starts (`waterSurface`'s `seen`), so the ground under it is lit and wetted as it is from far off, and
    its waterline lies flat (`wavySurface`), as its faces are drawn: before, the bed took a Pool's dappled light near
    the camera alone, and the waterline followed ripples the blocks' water does not have. Block size keeps the world's
    size: `BlockFile::set_block_size` counts its ground again in blocks of the new size, and what was built is read
    again in them (`edits::changed`, `edits::resized`: nearest for kinds, between the old blocks' middles for how full
    smooth ground is; a world without end's kept changes block by block). Tests: a sea's surface whole over a gentle
    shore, smooth sea generation, a pond's body of water, pillars and holes resized and back, a mound's surface kept,
    the editor halving Block size, the bed under a pond lit alike from near and far, a level waterline from just under a
    pond's top.
    In a world without end, water shows no face towards a chunk column not made (`BlockWorld::columns`,
    `blocks::mesh_made`): a coarse column is meshed with only itself made, near chunks with the near columns, and the
    water beside columns let go is meshed again; drawn smooth, the water's surface reads a column not made as the made
    one beside it, so it runs on flat. So a sea shows no walls where its rings meet. Every level's sea stands as high,
    so the rings meet without steps: of cubes where the finest level's does, a coarse column's top block of it only as
    full as that, in eighths (a flowing fill; coarse blocks never flow); drawn smooth at its sea level, a top block's
    surface standing up to a whole block over its middle (`surface_fill`). Tests: a streamed sea, cubes
    and smooth, with no water faces on its columns' sides towards columns not made and every level's top as high,
    water put in the air showing every side, a chunk of water showing its sides at a world's edge.
  - ~~**Blocks, items and recipes:**~~ done: a block is an asset of its own (`.block`, `blocks::block::BlockDef`): its
    look and liquid, which the engine reads, and its rules, hardness, the kind of tool, a tier, drops, sounds and a
    data asset of the game's, which scripts apply. A world's `[[blocks]]` list names its blocks by reference, keeps a
    colour alone as a kind of its own (as `.vox` models bring theirs), and leaves a place taken out empty
    (`BlockKind::block`, `removed`), so a block's number never changes and older worlds load untouched; ids are 16 bits,
    and a world draws 256 looks at most, refused past that rather than aliased. The engine keeps the file as written
    and draws from it resolved (`blocks::resolved`, kept on `BlockWorldAsset::drawn` with the blocks' version it was
    made at); a block saved is taken up by every world listing it in the next tick (`blocks::refresh`), remeshed only
    where its faces differ. A script may place a block a world does not list: the world numbers it in the game alone
    (`BlockWorldAsset::grown`), and the changes kept for saves and joining players say which block each such number
    is, after bytes that read as they always did. Rays pass through liquids unless asked, a block placed takes a
    liquid's place, and an item that leaves another holds a liquid (a bucket). C#: `Block`, `BlockHit.Definition`,
    `Blocks.Mine`, `Place` and `Scoop`, `BlockMiner`, events and `BlockRules`; `Item`, `ToolKind`, `Station`,
    `Recipe` and `Recipes.Match` as Definitions of the engine's own, described for the editor with a game's; every
    recipe ships (`[ContentRoot]` with no pattern on a Definition); a save keeps a data asset as which file it is; an
    `Image` shows a project picture in game UI. Editor: a Block's Inspector (Look, Breaking, Drops, Sounds, Data, Make
    Item), a Block World's list with Add Block, New Block, Add Color and Make Block Assets, brushes building through
    water. The inventory, its hotbar and screen, a crafting grid, a miner and a sample are a package of their own,
    `voxear-inventory`. Tests: an older world written back byte for byte, a place left empty, looks from blocks and a
    block saved taken up, shipping, captures old and new and renumbered, scripts mining by rules, placing unlisted
    blocks and reaching through water, recipes moved, mirrored and at a station, the Inspectors, and the package
    played in a game.
  - ~~**Blocks as the project's database:**~~ done: a world lists no blocks. Every `.block` of the project is one any
    Block World or Voxel Terrain can hold, and a world numbers a block the first time it holds one: in the editor as
    a brush first builds with it or its ground, its sea or a cover is first made of it (`block_props::number_block`),
    in a game as a script first puts it down (`blocks::block_number`). The numbering is the world's `[[blocks]]` as
    before, now its palette: kept in its file, shown nowhere, added to at its end and never renumbered, so older
    files and saves read as they did. What a game numbered since its file was read is what its saved changes name
    (`BlockWorldAsset::from_file`, `blocks_numbered_since`, in place of the list kept beside the file). A world's
    ground is made from its file cut after the last entry the ground can name (`BlockFile::for_ground`), so a block
    numbered later starts no world without end again. The 256 looks a world draws stay, as a quad names its look in
    eight bits; a world with no room says so once in the Console (`blocks::say_full`). Editor: the Block World's
    Inspector has no list, Add Block, New Block, Add Color or Make Block Assets; Block, Top block, Under block, Base
    block and Sea block pick from the project's blocks in the object picker, with thumbnails (a block's preview is
    a cube of it) and those built with last first (`Memory::recent_blocks`); Cover is the brush's block's. A
    **Blocks** panel (`block_database`, Window ▸ Blocks) shows every block as tiles made a row at a time, with
    search, New Block, Duplicate and Delete, which says which worlds hold a block and asks again. An older world's
    kinds made by hand become blocks in a Blocks folder beside it as it opens in the editor, its file written at
    once (`make_kinds_blocks`); a game still loads one as it is. Scripts: `Blocks.All` (native API 75). A build
    ships every block of the project. Tests: a file's ground unchanged by later numbers, a world numbering each
    block once and full at 256 looks said once, the brush numbering what it builds with as one step to undo, the
    picker's order, the Blocks panel's grid, search, copies and deletes, an older world made blocks and drawn as it
    was, every block in a build, `Blocks.All` in a game.
  - ~~**Biomes for worlds of blocks:**~~ done: a biome is an asset (`.biome`, `generate::biomes::BiomeDef`): its
    colour, the temperatures, humidities and, where it keeps to any, heights it lies in, and its ground's blocks by
    reference: top, under and how deep, deep, steep with its slope, shore with its height. A generated world lists
    its biomes in its `[generate]` (`BlockGenerate::biomes`). The engine reads them with the world
    (`blocks::with_biomes`), numbering the blocks they name in its palette, and gives generation the result
    (`BlockGenerate::placed`, no part of the file); `columns_at` gives each column the blocks of the biome its
    climate and height make it (`Biomes::at`: within its ranges or the nearest, one that keeps to a height first,
    then the narrowest, borders wandering by noise), for near chunks and the far view alike, the world's own
    blocks wherever a biome names none. A biome saved is read again, and every world listing it has its biomes
    placed again and its ground made again with its changes kept (`blocks::refresh`, and `Content::block_world`
    for one kept), in the editor and in play. Editor: a **Biomes** list under a generated world's generator rows
    (`block_panel`, `BlockField::Biomes`), with Add Biome from the picker or a drop, a grip and a cross, each one
    step to undo; a biome's Inspector (`biome_inspector`: Color; Preset of the eight climates, Temperature,
    Humidity, Height and Heights; the ground's blocks as pickers with a cross), saved as it is edited; New Biome in
    the Project panel's menu and the command palette; **Add Starter Biomes** (`starter_biomes`): Snow, Tundra,
    Taiga, Grassland, Forest, Desert, Savanna and Rainforest in a Biomes folder beside the world with fifteen
    colour blocks in its Blocks folder, a project's own blocks and biomes of those names used as they are. Tuned in
    renders of mountains and hills at 1 m and 8 m blocks: the Snow keeps to heights from 260 m in any but a warm
    place, since cooling 6.5 °C a kilometre alone leaves most peaks green, so mountains are white on top, rock on
    their steep faces, and low ground never snow. A block's Cover rows hide where a world lists biomes and work as
    they did where it lists none. Scripts: `Biome`, `Biomes.At(position)` (native API 78). Tests: generated ground
    by biome, biomes numbered as a world loads, a saved biome making worlds again, a place's biome, the list added
    to, moved, taken from and undone, the Inspector's fields saved, the starters made once and listed, snow by
    height through the editor, scripts asking a place's biome.
- **Smooth voxel terrain**, in parts: smooth ground you can see, stand on, generate and build on; a floating
  origin. Planets (ground all round a middle, with gravity towards it) were built here and taken out again.
  - ~~**Voxel Terrain:**~~ done: a Block World drawn smooth (`BlockFile::smooth`, `BlockWorld::smooth`). Each chunk
    keeps how full each block is where the surface passes (`chunk::Fill`, a byte a block, kept only for chunks that
    need it), generation writes it from the generator's depth (overhangs and caves too), and `blocks::smooth` meshes
    it by surface nets: a point a cube between eight blocks' middles where the surface crosses its edges on
    average, faces where ground meets air, the way the ground faces from the blocks round each point, points kept
    in 1024ths of a block so chunks meet exactly. The renderer draws a smooth world's triangles from the same pool
    (`BLOCKS_SMOOTH`, two pairs of words a corner), its kinds' top, sides and bottom blended by the way the ground
    faces. Brushes and `Blocks.Ball`/`Break` make balls whose surface lies between blocks (`smooth::ball`); saved
    changes carry fills (`VXBLOCKS` 2, 1 still read). Editor: Add Component ▸ Terrain ▸ Voxel Terrain on
    `Voxels/<Obj>.blockworld`, Smooth under Settings, its section titled Voxel Terrain. Tests: a surface where its
    fills say, a slope with no steps, chunks meeting at the same points, a closed blob, balls added, taken and
    painted, changes with fills read back and version 1 read, the GPU drawing a hill's top and sides with no gaps,
    a ball resting on it, a generated world on its generator's heights, the editor adding one and building a ball.
    An endless world's far rings meet too: a smooth world keeps its coarse columns' blocks, made a column further out
    than drawn, and makes a column's faces once the eight round it have theirs, so a ring's ground runs on across its
    columns; where a ring meets the finer one inside it, the coarser hangs skirts two blocks down its columns' sides
    (`smooth::mesh_skirted`), drawn and not stood on. Tests: skirts on the sides alone and facing out, coarse columns
    meeting point for point, the GPU drawing hills to the horizon with no sky through them. (Kinds blend within a triangle since the water blocks' second look, above.)
  - ~~**Standing against gravity:**~~ done: bodies and characters share gravity at a point (`scene::gravity_at`,
    `PhysicsScene::gravity_at`), the world's own everywhere. Each Character Controller stands against it, its
    capsule turned from its last up by the least turn (`vx_character_set_up`), its steps, slopes and floor measured
    along that up, and the move told that gravity. C#: `Physics.GravityAt(position)`, `CharacterController.Up`, and
    `SimpleMove` falling along it (native API 69). The First-Person View turns about the up where its Obj is: a
    level of its own (`FirstPersonView::level`, C# `Level`), carried by the least turn each frame, its Obj, camera,
    body twist and gun holder all following. The Motion Matcher measures its body's facing, the velocity wanted and
    seen and its clips' travel in a level of its own (`MotionMatcherState::level`), carried to its Character
    Controller's up or else against gravity; while gravity pulls straight down nothing is turned. Tests: a view
    under gravity turned sideways standing against it, and level again once it pulls down.
  - ~~**Floating origin:**~~ done, switched on in Project Settings ▸ World (`[floating_origin]`, off by default): once
    the viewer terrain streams round is past its distance (2048 m) from the world's middle any way, up and down too, the
    world moves back by whole steps (1024 m) between frames (`origin::due`, `Game::move_origin`); the sky and fog
    measure heights from the level's ground (`GpuFrame::origin.w`, `levelHeight`). `WorldOrigin` keeps where
    the middle is from the level's, in whole metres; root Objs move, those not moved since put back exactly from f64
    (`voxear_scene::move_objs`); with them tick smoothing, Jolt in one call (`vx_world_shift`: bodies without waking,
    characters with their contacts; joints held to the world held to a still anchor body), kept places and corners,
    sound (`Command::Shift`, acoustics measured from the level's middle), smokes, mists, motion matching, first-person
    eyes, gun holders and debug draw; the renderer's last-frame matrices, probes, tracks, foam, sea floor and foliage
    (`Renderer::shift_origin`); the sea's cascades phased by the origin in f64, drawn and for buoyancy. Streaming counts
    from its Obj, so nothing is made again. Mode Off, Endless Worlds (levels with ground without end) or
    Every Level. Networked: a server and a host keep the level's middle, each client moves its own; positions go between
    them measured from the level's middle, each machine adding its origin as it sends and taking it away as it reads, a
    root bit and a far bit each, 16 bits an axis more past 8 km (out to half a million km); clients' samples, spawns and
    prediction history move with them. Scripts (native API 70): `Origin.Moved`, `Origin.Position`, `ToLevel`,
    `FromLevel`, `LevelPosition`, and `[WorldPosition]` fields moved for them, and on `[Replicated]` properties and RPC
    arguments sent from the level's middle; saves and the editor's play keep positions from the level's middle; a level
    loads from its own. Tests: a ball 100 km out rolling within 0.02 mm of one at the middle (66 mm astray with the
    middle still), endless terrain and blocks carried on with nothing made again and physics on the drawn ground, the
    GPU's frame after a move matching the one before with motion blur on, scripts' fields and events, a prop back
    exactly after 100 km, players far out seeing, predicting and moving Objs 20–50 km out as the server has them, a
    player whose middle moved seeing and sending script positions from the level's. Patterns read from where things
    are measured from the level's middle (`GpuFrame::origin`, wrapped every 65 536 m; `levelPosition` wrapping each
    pattern every 1024, 4096 or 65 536 m): grass cells counted from it (`GpuGrassJob::corner`), wind, seasons,
    puddles, rivulets, lava and glitter, the ground blend and wet edges, foam, caustics, streams, lava lakes (one shift
    a Pool), underwater specks, spray, glows, the sun's cookie and its cascades' texels, smoke (from its corner) and
    mist, the foliage tint baked on the CPU, and custom shaders' new `levelPosition`. Tests: on the GPU, a patterned
    scene after a 3 km move matching the picture before it, and 131 km out matching it near; cascades' texels kept.
    Not done: a server exact for players far from the level's middle; GI probe spacings other than powers of two
    placed again on a move.
- ~~**Faces of small cubes:**~~ done: a Block World's **Voxels** (how many small cubes along a block's side) and
  **Relief** (how many deep the deepest sink), `voxels` and `relief` in its file, `BlockVoxels` on its look. The
  cubes' faces are shaded as a grid of small cubes over the whole world (`blockCubes` in `blocks.slang`): one
  colour each, the picture's there; sunk where the picture is darker than it is all over, or by a hash of their
  place where a face has only a colour; the ray walked across the grid to a top or a side, twelve small cubes at
  most. The mesh, shadows, depth and collision are unchanged, and the faces go plain where a small cube is under
  a pixel. About 0.1 ms a frame at 640×360 with faces near. GPU test: small cubes near, finer with more, flat
  without relief, the same each time, and plain from far above; file and editor tests of the two rows.
  - Later: a block's own relief, small cubes that stand out from a face and change its outline, their shadows,
    small cubes on a Voxel Terrain.
- ~~**Speed of block worlds and Voxel Terrains:**~~ done, measured on a test project with a level of each kind
  (`vox render --level --profile --fly`; the stats overlay and the Profiler count block triangles in view and in
  shadows). Each chunk's mesh for the GI's rays waited on the GPU about seven times; a frame's are now uploaded and
  built in one batch (`Gpu::batch`), keep no meshlets, and share their corners; a full mesh pool leaves chunks out of
  the GI rather than failing the frame. Physics makes chunks' shapes on
  every core and keeps a body whose triangles did not change. An endless world makes its near blocks a column past those
  drawn and meshes each column once, in the background, when the columns round it have blocks, so a flight across it
  spends 53 ms streaming where it spent 861 ms, its ticks 0.3 ms where 1.9, its slowest frame 33 ms where 130, and the
  near window's edge shows no walls. Smooth surfaces are merged where flat (a tenth of a block), the points beside
  another kind and those chunks share kept, and twice more for far away, a view drawing the coarsest its distance allows
  (a Voxel Terrain level: 146k triangles where it drew 265k, its sea 21k where 62k), and passes over chunks facing away; smooth water is merged as the ground is; a
  chunk changed is made again at its first level alone. Cascades draw only cubes' faces turned towards the sun (about
  half). A world whose faces did not change costs physics and the renderer no walk over its chunks. Small blocks: a
  block is 0.1 m to 10 m a side (`file::SMALLEST_BLOCK`, `LARGEST_BLOCK`; at 0.01 m a 64 m world was 4096 blocks
  across), and in the editor a world too large to make within a frame is made again on other threads, shown as it was
  until then and put in as one edit (`block_props::Making`): Voxel Terrain set to 0.4 m blocks froze the editor for
  2.9 s, and now its frames go on and the new world comes over about 14 frames, the slowest 33 ms: a pool grows
  once for every chunk coming (it staged 474 MB for 220) and takes 16 MB of them a frame, the old pool drawn until
  the new one has them all. Costs: a smooth world loads slower (three levels a chunk, Voxel Terrain 34 ms
  to 67). Not done: the coarse rings made and kept on the GPU, as nothing but drawing needs them; coarser levels past
  the chunks' fixed sides (resampled blocks); a coarser mesh for the GI's
  rays (very large worlds still fill the shared mesh pool); a frame's draws merged across a chunk's levels; ground
  generated per metre rather than per block, as noise for caves and overhangs is worked out for every block under
  the surface.
- ~~**Budgets for making chunks:**~~ done: what making worlds of blocks may spend is a setting
  (`voxear_terrain::WorldBudget`, `[world]` in `voxear.toml`, Project Settings ▸ World ▸ Block Worlds, `World.Quality`
  and the numbers under it for scripts): how many threads make terrain at once (a gate on the worker pool, so it
  changes as the game runs, heightmap tiles too), how many chunk columns a world makes at once
  (`BlockStreamer::at_once`), how many megabytes of chunks go up a frame, shared by every world, and the most view
  and far distance. Presets Low to Ultra, Low making two columns at a time on two threads; a number changed makes it
  Custom. Tests: Low never has more than two columns in flight, raising it as a world streams makes nothing again;
  a pool never works more jobs at once than its limit; the file, the editor's page and a script set it.
- ~~**The far view:**~~ done: from the third ring out, a world without end's ground is kept as columns of runs, each a
  block from one height to another, made from the generator's heights, climate and sea alone (`blocks::generate::far`,
  `FarTile`), the same blocks as a coarse ring's chunks wherever the ground is plain (tested chunk for chunk, cubes and
  smooth), made into chunks and meshed as the rings are, so the renderer draws them unchanged. Eight levels at most
  (`stream::MOST_LEVELS`), 16 km with metre blocks; each tile kept compressed in `.voxear/FarView/<world>/<what it
  was made from>/` (500 tiles for 16 km, 4 MB), so ground seen once comes back at once. What was built shows in every
  coarse ring where it fills an eighth of a ring's block, its rings made again once the camera leaves it
  (`stream::raise`); `FarTile::add` takes more blocks into a column, for structures and trees later. Measured on a copy
  of TerrainTypes' Endless Block World: to 4 km before and after alike (the same pictures, frame times within noise);
  16 km costs 118k triangles in view against 92k at 4 km, 636 MB against 556 MB, and is made by frame 192 against 148
  (a second), a second visit by frame 155. The presets reach 1 km on Low to 16 km on Ultra. Tests: a far tile's chunks
  against a coarse column's, its bytes and blocks added, tiles read back from disk, a tower built near kept in the
  rings, and mountains on the horizon 16 km off with no sky through the ground. Not done: a dithered overlap between
  levels in place of skirts; overhangs and caves far off (the manual says so); ground taken out showing far off.
  A far column of cubes takes water wherever its ground lies under the sea, its top block at least an eighth full or
  the ground's own top block the sea's, as 64 m blocks and more rounded a shallow sea away to its floor.
- ~~**Maps:**~~ done: a world of blocks drawn as a small model of itself, a minimap on the HUD and a map opened over
  the game. A map's tiles are the far view's (`blocks::map`, `map_tile`), 32 by 32 columns at any level, each cut
  down to its ground with two layers under its top, its water, and two pieces standing free of it, with what was
  built and dug put in: block for block at 1:1, coarser as the far view shows it; a world whose chunks are all held
  is read from its blocks. Nothing is meshed: one shader (`shaders/map.slang`, `voxear_render::map`) walks each
  pixel's ray through the columns, over groups of 8 and 32 cells none reaches up to, so hills, cliffs and the slab's
  cut sides with their layers are blocks, lit by one lamp with corner shade and short shadows. Each view keeps a
  window of tiles on the GPU, moved a tile at a time, and is drawn only when it changed. Measured on an RTX 3070,
  the least of sixty frames: a 256-pixel minimap 0.2 ms; a map of mountains 496 cells across at 2560x1440 1.7 ms,
  1.2 ms without shadows. A `Map` component puts both on the screen, opened by an input action; `Minimap` and `Map`
  elements go in a game's own UI; a `Map Marker` marks an Obj, `Map.AddMarker` a place. What a map knows is kept
  apart from the world (`blocks::map_known`, `MapKnowledge`): ground seen and layers of news with when each was
  heard, any number of them, joined newest first, 2.7 KB for a four-kilometre walk; ground not known is drawn as
  paper. Tests: tiles against the far view, a tower and a pit, the shader's pictures, the game's minimap and map
  drawn, markers, knowledge and its bytes, the C# API on a copy of Hello. Manual: Maps and Minimaps. Every view has
  seventeen settings by name (`MapViewSettings`: its size on the screen, pointer control, zoom limits, shadows, the
  parts shown), set in markup, from C# or on the Map; a `MapCamera` reads and steers how it is seen, and gives the
  place in the scene under a point of it; what the maps follow stays at the ground's edge once a map is moved off
  it; zooming keeps a map its size on the screen. Not done: heightmap Terrain as ground; trees and structures,
  which show once the far view carries them; turning the map with a gamepad without a script; a map keeping the
  ground as it was last seen.
- ~~**Plants a world grows:**~~ done: a Foliage under a Block World grows by itself on the world's ground
  (`[obj.foliage.grow]`, `scene_file::GrowData`): the blocks it stands on, temperature, humidity and height ranges
  (a Biome sets the climate at once), steepest ground, spacing, least apart, chance, patch size, sizes, aligned to
  blocks or free, and a seed. `voxear_terrain::blocks::grow` works a chunk column's plants out from that column's
  generated blocks alone, on the placement grid structures share, so a column made alone agrees with its neighbours
  and the same world grows the same plants on every machine. The engine (`grown_plants`) keeps each column's plants
  on a hidden Obj of its own under the world, drawn and collided as the Foliage's, made as the column's blocks are
  and dropped as it goes, within the Foliage's draw distance, eight columns a tick. A plant is gone while its block is
  dug, changed or built over, and back when the ground is: worked out from the blocks, so nothing is saved or sent.
  Past the world's own blocks, a column under a coarser ring that shows has the same plants from the generator's
  heights (`grow::grow_far`, tested plant for plant against the blocks), each kept clear of the ring's coarser
  ground, so trees drawn to 500 m stand on far hills and are the same trees once near. A plant a script takes away
  is kept on its Foliage by column and number (`voxear_scene::CutPlants`), stays gone as its column comes and goes,
  and travels with the blocks' changes in saves and to joining players.
  Six starter plants made of small cubes (`voxear_render::StarterPlant`: Grass Tuft, Flower, Bush, Rock, Tree,
  Pine), each a box model written into the project's Plants folder when picked. Editor: a Foliage's **Grow** tool,
  and **Add plant** on a Block World's Settings, which takes any model or a starter and makes the Foliage, one step
  to undo; a click on a grown plant picks its Foliage. Tests: the rule (same every time, a column alone, by block, climate, height and slope, separation,
  patches, gone and back), the engine on a world without end (two rules, the camera moved and back, digging and
  building, a second run the same), a GPU picture of all six starters, the editor (a starter picked, grown,
  changed, undone). Manual: Foliage, Block Worlds, Box Models. Woods in the far view: a Foliage's `far_block` shows
  its plants past where they are drawn as a wood of that block (`grow::Woods`), over the ground as high as they
  stand on as many of a ring's columns as would hold a plant, or as the top block's colour in rings of far taller
  blocks; from the first ring the plants do not reach (`Woods::first_past`), where the plants themselves stop.
  The streamer takes it through a hook any generated thing can use (`stream::FarBuilt`: a stamp, the first ring,
  and a tile to build into): far tiles are kept on disk without what is built into them, a nearer ring made in
  full takes it block for block from the tile its ground would be, and a change builds those rings again and no
  ground. Tests: a wood over grass and none over the sea, the hook (built in, changed, nearer rings, gone, the
  near chunks untouched), a GPU world whose far view has faces of leaves. Not done: the same rule on heightmap
  terrains.
- ~~**See-through blocks:**~~ done: a block drawn with a material that is cut out (`cutout` in its `.material`)
  is seen through, as leaves are. The engine reads it from the block's materials as a world's blocks are resolved
  (`BlockKind::cutout`, never written; `BlockFaces::see_through`), and again when a material is saved. Meshing
  (`blocks::mesh`, `blocks::hides`): a face shows against a see-through block as against air, a see-through block
  keeps its faces against its own kind, corners are still closed in by it, its faces are triangles to stand on,
  and a chunk of it through and through still has faces and is seen through by the cave culling. The block shader
  drops a face's fragments under its material's cutoff, in the depth pass, the shaded one and the sun's shadow
  pass (`blockShadowFragment`), so a crown's shadow is dappled. Tests: the faces of a tree of leaves against the
  same tree of solid blocks, the engine (read from the material, stood on), a GPU picture with the sky through a
  crown and lit ground in its shadow. Manual: Blocks. Not done: the same for a world drawn smooth, rays for
  bounced light, which take it as solid, and light through a leaf from behind (a material's foliage lighting is
  not read for block faces).
- Throughout: chunks culled against the frustum and last frame's depth, and the smallest save and network data
  (only changes from what a build or a seed makes, as commands while few and compacted deltas once many).

## M9 — Navigation, cameras and Timeline

- Navigation:
  - grid, layered grid, recast and point graphs
  - async paths, modifiers and runtime graph updates
  - RVO avoidance
- Camera Rig presets, the composable rig API, blends and shakes.
- Timeline.

## M10 — 2D and networking (in progress)

- [ ] Sprites and atlases, tilemaps with rule tiles, 2D lights, and the pixel-perfect camera.
- [x] The transport (`voxear_net`): encrypted, authenticated connections over UDP, joined with a connect token,
  with reliable and unreliable channels, and a simulator of bad networks.
- [x] Networked play as a server, a host or a client, in ticks: Objs shared with every player, RPCs, movement
  predicted and corrected, lag compensation, and shared authority.
- [ ] Multi-instance play mode.

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
- **A documentation pass** before 1.0, so the manual and the Code Reference are correct and clear:
  - every page checked against the engine as it is: each code sample compiled, and run where it can be, by a test;
    each setting, attribute, window and API it names checked to exist; each picture taken again from the editor
  - every topic laid out as Unity's and Unreal's docs are: what it is and when to use it, a worked example from
    scratch, then the reference, with the easy path first and the full control after
  - pages read by someone new to Voxear following them step by step, and rewritten where they get stuck
  - Docs Health in the editor holding the line after: pages older than the code they describe, samples that no
    longer build, and APIs with no page
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
