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
  - [ ] C# components (`IComponent`) marked to be kept in a save of the scene
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
    - [ ] still to come for drawing: solid shapes, wire meshes, widths in metres, and labels in a corner of the
      screen
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
    - [ ] `Handles.Disc` for turning, and `Handles.Position` with move arrows along each axis
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
    are untouched.
  - At half resolution by default: each frame searches one pixel of each 2x2, and the temporal pass, at full
    resolution, fills in the rest from what lies on the same surface.
  - Settings in `[post.ambient_occlusion]`: radius, strength, a tint for stylised looks, quality, and
    `full_resolution`.
  - An AO debug view (F7, or `--debug-view ao`).
  - On an RTX 3070 at 1440p it costs about 0.35 ms (Stress), and full resolution about three times that.
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
- Still to come for lights: shadows from point, spot and area lights; the C# `Light` API; and the sky's settings
  in the editor rather than only the scene file.
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
  and a top level per view and frame. Two cascades of 32×16×32 probes trace 128 rays each when due, shading hits
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
  first. After the sky, the picture is copied into an image with seven mips, and each pane is drawn over it: what
  lies behind, moved by the normal's tilt in view, blurred through the mips by roughness with a ring of samples,
  tinted by the colour and dimmed by Fresnel, plus the surroundings, the sun and the lamps reflected. The editor's
  material inspector has Glass and IOR, and the Material menu lists the built-in Glass. GPU tests: clear glass
  shows what lies behind it, tinted glass colours it, rough glass blurs it.
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

Next:
- Reference look: The Finals and Arc Raiders (Embark), for bounced light, exposure and contrast.
- Alpha-tested and two-sided materials in the GI's rays; probe rays for reflections.
- Virtual shadow maps, and shadows for point, spot and area lights.
- Optional third-party upscalers: AMD FSR, NVIDIA DLSS and Intel XeSS, each behind its own SDK.
- Baked lightmapper, light probes and reflection probes.
- Reflections: falling back to reflection probes rather than the sky, and tracing against the depth pyramid,
  for fewer misses on surfaces seen edge on.
- **Ray-traced audio** as an optional setting: GPU rays against the GI's acceleration structures, fed to Steam
  Audio, with baked reverb probes as the fallback.
- **HDR colour, end to end.** Rendering is HDR already: a float target, emissive and lights above 1, exposure and
  tonemapping. Still to come:
  - a C# `Color` type, with HDR intensity for fields such as `[ColorUsage(hdr: true)]`
  - HDR textures (`.exr` and `.hdr`) imported as half floats and cooked to BC6H, for skies, light probes and
    emissive maps
  - HDR display output: HDR10 and scRGB swapchains with paper-white and peak-brightness settings, and tonemapping
    that targets the display

## Vegetation (next, after glass)

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
  About 12,000 plants in the valley test: 6 ms a frame at 720p on an RTX 3070. Still to come: per-instance
  colour, picking plants in the Scene view, and scattering onto uneven ground.
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
  - LODs that fade into each other by dithering rather than popping, and grass and small plants that fade out
    by distance.
  - Impostors for far trees: octahedral pictures baked from the tree, turning to face the camera, lit as the
    tree is.
  - Shadows for foliage drawn from cheaper LODs, and none for grass beyond a distance.
- ~~**Wind:**~~ done: a Wind Zone component (strength, gusts, gust size and speed, turbulence) blowing along its
  Obj's +Z over the level. `shaders/wind.slang` moves every vertex in the forward, depth, shadow and mesh-shader
  passes, and last frame's for motion vectors: plants bend from their pivot up by height (from the mesh's
  bounds) and lean downwind on an arc, sway at a phase of their own, and their leaves flutter along their
  normals; gusts are two octaves of value noise drifting downwind, so a field shows waves. Materials have
  `wind_bend` and `wind_flutter`, 1 for foliage unless written; culling widens a moving plant's sphere. Editor:
  Wind Zone component with arrows; material inspector Wind bend and Leaf flutter. Still to come: masks from
  vertex colours, for branch sway apart from the trunk.
- ~~**Pushing through it:**~~ done: a Foliage Interaction component (radius, strength). The 16 pushers nearest the
  camera go to the frame; a persistent 128×128 grid of half-metre cells round the camera (toroidal, as the GI's
  probes) holds a push each, and **Foliage pushes** springs every cell back (to a third in 0.8 s) and stamps the
  strongest push outwards from each pusher. `wind.slang` reads it at each vertex, blended over four cells, and
  bends the plant away from the ground up, on an arc, with or without wind. GPU test: a pole leans from a pusher
  beside it and stands again once it has gone.
- **Seasons and variation:** a level-wide setting for the season (green to dry to autumn to bare to snow),
  colour variation across a field from world-space noise, dryness, and wet or snow-covered leaves from above.
- **Glow:** emissive masks on plants that pulse, breathe or flicker, for glowing mushrooms, bioluminescent
  forests and fireflies, and light from them in the GI.
- **Placing it:** a scatter brush in the editor, painting instances onto any surface with density, scale and
  slope limits, and erasing; terrain's scatter layers (M8) use the same instances.
- **Converting plants:** a step that prepares models from other tools (SpeedTree exports, Quixel and asset-store
  plants) for wind and foliage shading, writing the masks the shader reads.

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
- **Built-in terrain generation** *(later)*: generators that make a terrain to start from, chosen and tuned in the
  editor or run from code, each feeding the layer stack so it can be painted and stamped over afterwards:
  - noise-based heightfields (fractal, ridged, billow, domain-warped) with seeds
  - biomes and climate (height, moisture, temperature) choosing materials and scatter
  - island and continent masks, mountains, plateaus and canyons
  - rivers and lakes carved by flow, alongside the GPU erosion above
  - roads and paths that flatten the ground along a spline
  - generation at runtime, in chunks, for endless or procedural worlds, as well as baked in the editor
  - Later still: voxel terrain with caves and overhangs.

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
