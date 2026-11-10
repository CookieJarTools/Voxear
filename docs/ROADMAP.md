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
  foam on still shallows; faster water rougher, its ripples drawn out along the current, with low standing waves;
  rapids raised into standing waves and churn (`streamSwell`, the strip and its shading alike), white on the crests,
  pale with air, with spray the renderer adds as a particle system for each Stream with rapids
  (`streamSprayVertex`). What stands in fast water has a white collar and leaves rough white water downstream, and
  water fast over a shallow bed breaks white, both found in the picture's depth behind the water a few places
  upstream (`streamWake`). Tests: rapids white and the level stretch not, calm smooth against fast rough, white
  water below a rock in fast water and none in calm, spray gathered only for rapids.
- **Lava:** Pools and Streams of lava glow by Planck's law, yellow-white where it wells up to dull red. A skin sets
  on molten rock in small cells with bright seams; crust forms in plates, each from its middle out, that stand above
  the molten rock between them, seen down into past the near plate's edge (`lavaParallax`, worked out from the plates
  themselves); gaps narrow to cracks and the crust folds along its edges. A flow stands as high as it is deep with
  rounded sides and ends, crusts from its banks in on a molten channel, its plates placed by when the lava set out,
  with pahoehoe ropes bowed at the banks. A lake turns over in wandering patches. Bubbles swell into domes, break
  the crust, burst and leave a ring that heals, throwing up blobs of molten rock that cool as they fly; vents spit
  one burst after another. All of it is worked out from place and time, alike on every machine. Lava lights what is
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

## M6 — Animation

- GPU skinning, blend shapes, state machines, blend trees, layers and root motion.
- Motion matching: the feature database, then search, then inertialisation, then GPU crowd search.
- IK.

## Gameplay kits: game-ready systems

Systems a game needs again and again, ready to add to an Obj and play, so a new project moves in minutes rather than
weeks. Each is a component with sensible defaults as the easy way in, and every part of it open as the advanced
one: settings to tune in the Inspector, events to hook, and the code itself to read and change, since a kit is
plain C# in a package (see Packages). They build on input and physics (M4) and, for characters, animation (M6).

- **Gun kit:**
  - hitscan and projectile weapons, fire modes (single, burst, automatic), ammo, magazines and reloads
  - procedural recoil: kick and climb from curves, recovery, and camera and weapon sway, so guns feel weighty
    with no animation authored
  - spread that grows with firing and moving and settles when still, aim down sights, and bloom
  - impacts by surface (decals, particles, sounds), muzzle flash and shell ejection
  - damage with falloff and hit zones, and an interface for anything that can be hit
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
- Shader Graph with previews and sub-graphs, plus material variants, writing custom shaders.
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
- Particles, still to come:
  - the IR compiler, which promotes a system off the stateless path when it needs state
  - the preset gallery
  - stack mode, then graph mode
  - depth collision, ribbons and flipbook baking
  - the particle debugger

## M8 — Terrain

Heightmap terrain first, then digging, generation, block worlds, and smooth voxels and planets: each its own
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
    hashes. The Generate layer asks for a padded tile at once, block worlds for a chunk column, planets for a row.
    A graph with a loop or no Output still makes ground and says what is wrong (`WorldGen::faults`). Inspector:
    a landmass's settings as before; any other graph, its seed, scale, sea level and exposed rows. Tests: an old
    file's hash, a landmass in a graph against itself flat and round, batches against single places, text read
    back as written, the hash following the ground alone, broken graphs, block chunks by hash before and after.
  - the graph editor: a canvas that pans and zooms, typed ports and wires, search to add a node, undo; the
    Inspector for the picked node, values exposed from their rows; a preview on every node and a 3D preview that
    refines while a value is dragged
  - more nodes (cells, falloff, warp, terrace, blend, smooth; masks from height, slope and curvature; gullies as a
    node of their own) and the five presets rebuilt as graphs
  - biomes as nodes: where each is and what it brings (its shape, a Terrain's surfaces, a Block World's blocks),
    laid over one another as layers; surfaces' climate ranges and kinds' covers moved over
  - voxels: a Volume beside the height, with overhangs, tunnels and chambers as nodes; hillsides no longer pocked,
    layers counted from the real surface
  - cave rooms as prefabs of shapes and exits, strung together by a Cave Network node, as a bounded cave level or
    under a world without end
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
  Terrain. Not done: trees scattered by rules (a Foliage's plants by surface or biome).
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
  - trees and other plants scattered by surface and biome
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
    (`BlockGenerate::sea`; a planet's sea its radius). A Voxel Terrain kept its water as cubes (smooth since, below),
    the smooth ground running under it, each block of water taken as a block further out than the ground beside it, its
    sides drawn against anything not water; balls add ground in place of water and never take it. Editor: each kind's
    Water (its materials' rows hidden), and Sea block under generated ground's settings. Tests: water's faces apart and
    the ground's under it, a chunk of ground beside one of water, sights through water, water over smooth ground, a
    generated sea up to its level, water's top in a column, a ball floating and one sinking to the floor, the GPU
    drawing a pond over its floor, the editor making a kind water, placing it and picking a sea block.
    Flowing (`blocks::flow`): water placed and a generated sea are sources, full for good; water runs from them
    down, then out over ground an eighth of a block lower each block, seven at most, falling as a column off an
    edge, never running out from falling water or over water, and dries a step at a time once nothing feeds it. On a
    planet, down is towards its middle along the axis a block lies furthest out along (`flow::Fall`). A flowing
    block keeps its level and its way up in its fill (`flow::Water`), so saves and the network carry it; the mesher
    lowers its top and sides and draws the strip over lower water, each such quad keeping its shape where the
    corners' shading was and its way up in its first word's top bits (`mesh::water_shape`), which `blockWaterVertex`
    moves its corners by. A step looks only at blocks near the last step's changes (`BlockWorld::flowing`, set by
    every change), each from the blocks as they were, so every machine flows the same; `voxear_engine::blocks::flow`
    runs five a second in `Game::tick`, editing and playing, at most four a frame. Tests: levels, reach, falling off
    an edge, drying, a Voxel Terrain planet's water running round it, a step a block from the blocks as they were,
    quads' shapes, the engine's steps the same however frames fall, the GPU drawing water lower the further it runs,
    the editor's water running out and undone with it. Buoyancy pushes against the pull where a body is, as strongly
    as it pulls (towards a planet's middle near one, as physics pulls), reading how deep it is along the water's own
    up (`BlockWorld::water_depth`), so things float on a planet's sides; tested with a ball floating out from a
    planet's middle and one sinking to its stone. Not done: an under-water view on a planet's sides; scripts'
    `Water` finding a planet's water off its top.
    After a first look: still water spreads over water (falling water only over ground), so a lake runs into a hole
    dug beside it, and a block between two sources becomes one; a Voxel Terrain's water is smooth, its surface made
    by surface nets over a field of water's and ground's depths, faces only where water meets air, a generated sea's
    top blocks keeping where its surface lies (`flow::surface_fill`, `smooth_surface`), so it lies flat at its sea
    level and round a planet; block brushes work once a click, again only as the pointer moves half a brush on, and
    while playing; terrains' and block worlds' looks count as using their materials, so their colours survive play
    (they were freed as a level loaded). Tests: a lake running into a hole, a gap filling, smooth water at its top and
    at a sea's level, a block world's colours kept loaded.
    After a second look: drawn smooth, a generated sea takes the ground's top block where the ground lies under it with
    no block of sea over it (`generate::pushed`; the ground below stays, the sea's floor), and its surface runs a block
    into the shore (`smooth::water_surface`: ground beside the water's top with air over it takes the water's level and
    a face, up being the map's, `BlockMap::round` on a planet), so it meets the ground with no gaps; each smooth corner
    takes the kind of the ground nearest the surface among its eight blocks (`Padded::kind`) and a triangle's three
    kinds blend by how near each corner a fragment is (`blockVertex` packs them in `lod` and `cluster`), which also
    keeps dirt from showing through a thin top layer on slopes. Under water in blocks: the water round the camera is a
    body among the Pools (`water_bodies::block_water`, `WATER_BLOCKS`), level with the top of the camera's column and as
    far as that top runs along x and z, in the kind's colour (`BlockFaces::color`), so the under-water view, the
    waterline and the wet lens are a Pool's; it draws no surface or walls and leaves the sea alone. Block size keeps the
    world's size: `BlockFile::set_block_size` counts its ground again in blocks of the new size, and what was built is
    read again in them (`edits::changed`, `edits::resized`: nearest for kinds, between the old blocks' middles for how
    full smooth ground is; a world without end's kept changes block by block). Tests: a sea's surface whole over a
    gentle shore, smooth sea generation, a pond's body of water, pillars and holes resized and back, a mound's surface
    kept, the editor halving Block size.
- **Smooth voxel terrain, flat and planets**, in parts: smooth ground you can see, stand on, generate and build on;
  planets with their own gravity; a floating origin.
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
    Not done: levels of detail that meet (far rings are drawn smooth but meet with seams). (Kinds blend within a
    triangle since the water blocks' second look, above.)
  - ~~**Planets:**~~ done: generated ground as a planet (`BlockGenerate::planet`, `BlockPlanet { radius }`) all round
    a middle at its world's corner, its generator read round it in three dimensions (`WorldGen::height_round`,
    `climate_round`, colder towards the poles; fields read through a flat or round `Spot`, `noise::fractal3`), once,
    on a cube's six faces about a block apart and read between (`blocks::generate::Round`); chunks deep under its
    caves its base block whole, the rest block by block on every core; level with its sea without a generator.
    `BlockWorld::planet` gives physics its radius: moving bodies within four radii fall towards its middle in place
    of gravity (`PhysicsScene::pull_to_planets`). Editor: Ground ▸ Planet and Radius. Tests: a planet on its
    generator's heights out every way, no step across a cube's edges, colder poles, a ball falling onto a planet's
    side, the GPU drawing it round and lit on one side, the editor making one and undoing it. Not done: erosion's
    gullies on planets.
  - ~~**Walking round planets:**~~ done: bodies and characters share gravity at a point (`scene::gravity_at`,
    `PhysicsScene::gravity_at`): towards the nearest planet's middle within four radii, the world's gravity
    elsewhere. Each Character Controller stands against it, its capsule turned from its last up by the least turn
    (`vx_character_set_up`), its steps, slopes and floor measured along that up, and the move told that gravity.
    C#: `Physics.GravityAt(position)`, `CharacterController.Up`, and `SimpleMove` falling along it (native API 69).
    The First-Person View turns about the up where its Obj is: a level of its own (`FirstPersonView::level`, C#
    `Level`), carried by the least turn each frame, so its yaw goes round with it, its Obj, camera, body twist and
    gun holder all following. The Motion Matcher measures its body's facing, the velocity wanted and seen and its
    clips' travel in a level of its own (`MotionMatcherState::level`), carried to its Character Controller's up or
    else the pull where it is: its body stands away from a planet's middle whatever its Obj's turn, and root motion
    presses it to the ground along that up; off planets nothing is turned. Tests: a character dropped beside a planet
    landing on its side, upright to it, and walking round it; a view on a planet's side turned about its up and its
    yaw carried over the top; a script doing the same through `GravityAt` and `SimpleMove`, its view and Obj upright;
    a motion-matched character walking round a planet's side by its clips, its body upright to it and facing the way
    it goes.
  - ~~**Floating origin:**~~ done, switched on in Project Settings ▸ World (`[floating_origin]`, off by default): once
    the viewer terrain streams round is past its distance (2048 m) from the world's middle across the ground, the world
    moves back by whole steps (1024 m) between frames (`origin::due`, `Game::move_origin`). `WorldOrigin` keeps where
    the middle is from the level's, in whole metres; root Objs move, those not moved since put back exactly from f64
    (`voxear_scene::move_objs`); with them tick smoothing, Jolt in one call (`vx_world_shift`: bodies without waking,
    characters with their contacts; joints held to the world held to a still anchor body), kept places and corners,
    sound (`Command::Shift`, acoustics measured from the level's middle), smokes, mists, motion matching, first-person
    eyes, gun holders and debug draw; the renderer's last-frame matrices, probes, tracks, foam, sea floor and foliage
    (`Renderer::shift_origin`); the sea's cascades phased by the origin in f64, drawn and for buoyancy. Streaming counts
    from its Obj, so nothing is made again. Mode Off, Endless Worlds (levels with ground without end or a planet) or
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
    player whose middle moved seeing and sending script positions from the level's. Manual: Floating Origin. Not done:
    shader patterns from absolute positions (grass, wind, seasons, puddles), moves up and down (sky and fog heights), a
    server exact for players far from the level's middle.
- Throughout: chunks culled against the frustum and last frame's depth, and the smallest save and network data
  (only changes from what a build or a seed makes, as commands while few and compacted deltas once many).

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
