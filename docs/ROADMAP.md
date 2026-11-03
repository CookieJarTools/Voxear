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

## Packages: sharing code, tools and content

Voxear is meant to be open: a project's own code already adds editor windows, side bar buttons, commands, drawers
and Inspector attributes. Packages carry that from one project to another, as Unity's packages do, so people can
share tools, systems, shaders, art and samples and add them to a project in a click.

- **What a package is:** a folder with a `package.toml` (name, version, description, authors, the engine versions
  it works with, and the packages it needs) holding any of: C# scripts, built into an assembly of its own that the
  game's scripts can use; editor extensions (windows, side bar items, drawers, commands); assets; VoxDocs pages,
  which the editor shows beside the manual; and samples to copy into a project.
- **Where packages come from**, written in the project's `packages.toml`:
  - a Git repository, GitHub first, by URL and a tag, branch or commit: `orbit-tools = { git =
    "https://github.com/someone/orbit-tools", tag = "v1.2.0" }`, with `path` for a package inside a larger
    repository, as Unity's Git URLs work
  - a folder on disk, for packages being written alongside a game
  - a registry: a plain index repository on GitHub that lists packages and their versions, for finding them by
    name and searching, with more registries addable later
- **Resolving and locking:** versions are semantic, ranges are resolved together with every package's own needs,
  and `packages.lock` keeps the exact commit of each, so everyone on a team gets the same code. Downloads are
  cached once per machine and shared between projects.
- **The Package Manager window:** what a project has, with updates; add by Git URL, by folder or from a registry;
  remove; read a package's pages; import its samples. A package from the internet runs code, so the first time
  one is added the editor shows what it will add and asks.
- **From the command line:** `vox package add <git url or name>`, `vox package update`, `vox package remove`, and
  `vox package new` to start one, with `vox package publish` to list a tagged release in a registry.
- **Later:** native plugins (Rust or C libraries a package loads into the engine, behind the same consent), and a
  place to browse packages with pictures and ratings, as Unity's Asset Store.

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
  - [ ] dates and currencies in smart strings
  - [x] binding through `@key` in UI markup and in an element's `Text` from a script, shown again when the language or
    the text changes
  - [x] `LocalizedString` fields, picked in the Inspector from the project's keys beside their text, saved as the key
  - [ ] a `LocalizedText` component (once there is text in the world), and observable `Localized(key)`
  - [x] CSV export, and import from a file with what it adds, changes and takes away shown first (`vox loc export` and
    `vox loc import`)
  - [x] CSV export and import from the editor: Export CSV and Import CSV under a string table in the Inspector, with
    what it adds, changes and takes away shown before Apply
  - [ ] CSV import from a URL
  - [x] pseudo-localisation from the palette, accented, a third longer and in brackets, while editing and playing
  - [x] string tables edited in the Inspector: each entry's key, its text in each language and its comment, missing
    text and text past its limit marked, entries dragged into a new order
  - [x] a Localization Tables window: every table's keys against the languages, with search, find usages, and
    renaming a key everywhere it is used
  - [x] the saved choice and Windows' languages in language detection: the language a script picks kept beside the
    saves and started in next time, and Windows' preferred UI languages
  - [ ] the store's language (Steam's) in language detection
  - [ ] RTL mirroring and per-locale fonts
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
  - [ ] Steam Audio convolution reflections. Ray-traced audio follows in M5, once the ray-tracing acceleration
    structures exist.
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
- **Built-in vehicle physics**, on Jolt's vehicle constraints *(later, once the physics above is in)*:
  - wheeled vehicles (cars, trucks, motorbikes) with suspension, tyre friction curves, engine and gearbox, and
    differentials
  - tracked vehicles (tanks)
  - a `Vehicle` component with presets as the easy path, and every part (wheels, engine torque curve, gears,
    anti-roll) open to tune or drive from code as the advanced one
  - input bindings for driving, and a debug view of wheel contacts, slip and suspension

## M5 — Lighting

- Per-meshlet culling (frustum, cone, Hi-Z) on mesh shaders with a compute fallback, including shadow passes.
- Area and tube lights, IES profiles and light cookies, sky and atmosphere.
- Volumetric fog: a froxel volume lit by every light, with the sun casting volumetric shadows, so light shafts show.
- Soft shadows (PCSS, with a softness per light) and contact shadows, then virtual shadow maps.
- Upscalers (FSR built in, DLSS and XeSS optional), colour grading LUTs, and local post volumes.
- **Real-time voxel GI** (clipmaps, then injection, gathering and the radiance cache).
- Baked lightmapper, light probes and reflection probes.
- Ambient occlusion (GTAO), with a strength and an optional tint.
- Reflections: screen-space reflections, falling back to reflection probes.
- Stylised shading: shadows tinted a colour, and foliage shading (wrap lighting, light through the leaves, and a
  rim light tinted by the sun).
- **Lighting looks, as presets.** A scene picks a look, which sets the sky, sun, shadows, fog, reflections and
  post-processing together, and any part of it can be changed.
  - **Soft Overcast**, the engine's default look: hazy, bright and calm, like daylight through big windows on a
    cloudy day.
    - The sky is the main light, cool at about 6500–8000K. The sun is weaker and warm, about 5000–5500K, giving
      2 to 4 times the sky's light rather than 10, with soft shadows.
    - Thin, slightly blue height fog, and volumetric fog with the sun's shadows in it, so light shafts show
      (strong forward scattering: a Henyey-Greenstein g of about 0.6–0.75).
    - Auto exposure with a narrow range, set for interiors, so bright windows blow out and bloom into the frame.
    - Ambient occlusion at moderate strength, so contacts stay soft and clean; reflections on surfaces of low to
      mid roughness by default; and GI that brings sky light indoors, without which interiors go flat and dark.
    - A soft filmic finish: AgX or ACES with a gentle highlight rolloff, low, wide bloom with no threshold,
      saturation around 0.9, shadows graded slightly toward teal and highlights slightly warm, and a light
      vignette.
  - **Storybook**, a stylised look: a warm sun breaking through a canopy into a cool forest, with rich colour and
    soft shapes, as in It Takes Two, Kena or Ori. Art-directed rather than physically accurate.
    - A fairly strong, warm sun, about 3500–4500K and pushed toward yellow-green, against a teal sky light: a
      gradient with teal above and warm brown below.
    - Shadows that are never black or grey, but tinted warm brown, olive or purple, with soft edges, and a dappled
      canopy pattern on the sunlight.
    - Foliage lit from inside: wrap lighting, light through the leaves, and a rim light tinted by the sun.
    - Teal-cyan fog that separates the scene into two or three depth layers; volumetric light only faintly, if at
      all.
    - Stronger, tinted ambient occlusion; rough surfaces by default (0.6–0.8) with less specular.
    - Moderate, wide, slightly warm bloom, saturation about 1.1–1.2, a gentle vignette, optional depth of field
      on the near foreground, and grading that lifts shadows toward teal or purple and pushes highlights toward
      gold.
  - What they need: the voxel GI, probes, volumetric fog, soft shadows, ambient occlusion, reflections, light
    cookies and stylised shading above. Most of their post-processing exists already (AgX, ACES, bloom with no
    threshold, auto exposure, grading, vignette and depth of field); bloom still needs a tint.
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
