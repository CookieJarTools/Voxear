# World building tools for Block Worlds

A plan for building worlds that are generated first and crafted on top, written 2026-11-20 from CubeGame's spec
(engine 0.1.1-nightly.20261119.3). Nothing here is built yet. It covers what the engine does now, the one new
idea underneath (world layers), how each tool sits on it, answers to the spec's open questions, and the stages.

## What the engine does now

- **A chunk column is made in one pass.** `column_chunks_within` (`voxear_terrain/src/blocks/generate/columns.rs`)
  asks the generator for every column's height at once, works out biomes, reads overhangs and caves, adds water,
  then builds structures (level 0 only). Chunks are 32 blocks a side.
- **Hand edits are differences from that "base".** `BlockEdits` keeps, per chunk, what differs from the made chunk.
  The streamer applies a column's pending edits when it arrives and diffs it back into pending when it goes.
  Anything that changes the ground must therefore be part of what base is made as, or saved edits stop lining up.
- **There is no local remake.** A change to the world's settings or generator remakes every column. The only
  partial path is the far view's `stale` set, used when built blocks change.
- **Far off, only placed blocks show,** and only once the camera has been near: far tiles are cached on disk
  without edits, `raise()` looks at placed blocks only, and edits count once their column has been let go.
- **Structures already fit the ground**: `GroundFit { Leave, Foundation, Flatten }` with a sink, column by column
  under the footprint, with sheer edges and no clearing of plants.
- **Terrains already have layers that remake locally.** Each layer reports the rectangle it reaches; a change
  dirties the old and new rectangles and only the tiles under them are composited again, in parallel.
- **Navigation follows chunks by pointer.** Block nav rebuilds a chunk whose `Arc` changed, so anything that
  replaces chunks is picked up. Meshes are only in the baked level nav, not in block worlds.
- **Nothing tells anyone that a column came or went.** Grown plants and crowd records each find out by diffing
  the made columns every frame.

## World layers

A world layer is data on an Obj that changes what the ground comes out as, without storing blocks.

**Order.** The ground of a column is worked out as:

1. the generator's ground (with painted maps feeding its nodes, stage 4);
2. world layers, in order of kind (ground fits, then roads, then area fills), and within a kind by priority,
   then by the Obj's scene id;
3. hand edits from the `.blocks` file, on top.

**Shape of a layer.** Every layer answers, per column, from where the column is and nothing else:

- whether it reaches the column (a bounding rectangle, as Terrain layers do);
- a new top height, or "leave it";
- which blocks to fill with and top with, and whether to clear above (cut) and how high;
- whether generated plants and trees are left out there.

This works on whole columns after the generator's heights and before structures and water, so the volume's
overhangs above a cut are cleared and caves under a foundation are filled.

**Index.** A `WorldLayers` set, shared by `Arc`, is handed to the streamer with the world. It keeps the layers by
32-block column, so making a column asks only the layers that reach it; a capital city of thousands of fits costs
a lookup per column. Its hash goes into neither the world hash nor the far-tile folder: changing a layer must not
remake the world.

**Remaking only what a change touches.** A changed, moved or deleted layer dirties its old and new rectangles.
For each made column under them, off the main thread:

1. the column's hand edits are taken as a difference from its old base;
2. the column is made again with the new layers, as its new base;
3. the edits go back on. A block the designer placed by hand stays.

The old chunks are drawn until the new ones are meshed and lit, as Terrain layers do. Columns not made yet need
nothing: they will be made with the new layers when they come.

**Far view, map and minimap.** Far tiles stay cached without layers. Layers are applied to a far tile's runs when
it is loaded, and again when a layer over it changes (the tile goes into the `stale` set). The map's tiles do the
same. Cheap, because a layer changes runs per column, not cells. The Fantasy City's raised quarters then show at
any distance without a visit. Hand edits far off are a separate, older limit; they are not part of this plan.

**Turning layers off by kind.** A view setting in the editor makes the streamer use the world without those kinds,
to see the bare ground. It never changes the scene.

## Unloading with the ground

Asked for by the user alongside the spec: when a chunk unloads, the house and objects on it unload with it, and
the same API unloads spawned mobs.

- **Column events.** The streamer records the columns that came and went each frame (`take_finished` and
  `let_go` already know). The engine exposes them: `BlockWorld.ColumnsCame` / `ColumnsLeft` events in C#, and
  `blocks.columns` in `vox editor call`. Grown plants and crowd records move onto these instead of diffing.
- **Streams With Ground component** on an Obj, with a typed When Unloaded choice:
  - *Keep for later* (default for scene Objs and prefabs with a Ground Fit): the Obj and its children are taken
    out and kept as a record, the way far Crowd Members are, and made again exactly when the column comes back.
  - *Destroy* (default for spawned mobs): the Obj goes and does not come back.
  - *Stay*: nothing happens (for things that must stay loaded, such as quest objects).
- The column an Obj belongs to is found from its position when it is placed, spawned or moved; a Ground Fit's
  footprint centre wins when it has one. A world setting gives the default for Objs without the component.
- Scripts get `OnGroundUnloading` / `OnGroundLoaded` messages on the Obj, so a mob can save its state first.
- Saves keep the records, so a level's Objs far from every player cost nothing at run time.
- In the editor, nothing unloads unless a preference asks, so editing a city far from the camera is safe.

## The tools

### Ground Fit (stage 1)

Component on an Obj, so on a prefab's root and every instance.

- **Footprint:** a list of boxes or an outline, in the Obj's own space. The easy way is one box filled from the
  Obj's model bounds when the component is added.
- **Mode:** Foundation, Flatten, Cut. The same enum as structure placement's Ground setting, which gains Cut.
- **Level:** the Obj's own height, or a point (a child Obj, picked, such as the door).
- **Sink**, **Margin**, **Slope out** (0 for a retaining wall), **Fill** and **Top** (block pickers, None for the
  biome's own), **Clear plants**, **Priority**.
- **Slope out is walkable by construction:** at `d` blocks past the margin, the height is the generator's
  clamped to within `d` of the level, a block a step, until `d` reaches Slope out, where it stops.
- **Snapping:** placing or dragging an Obj with a Ground Fit in the Scene view lands it on the ground made
  without its own fit.
- **Gizmo:** footprint, margin and slope-out ring, drawn on the ground.
- **Script access:** `Obj.GroundFit`, `AddGroundFit()`, and every setting. `vox editor call` uses the existing
  `component.add` and `field.set`; one call `ground.fit.drop` lands a fitted Obj on the bare ground.
- Generated structures can use the same code for their own ground, gaining slope out.

### Roads and Area Fill (stage 2)

- **Road component:** spline points (dragged in the Scene view, dropped onto the ground), width per point, palette
  (a weighted random mix hashed by block, or a Pattern asset: a small grid of block references repeated along and
  across), edge block and kerb height, Ground (Follow or Grade with a steepest slope and slope out), Steps (blocks,
  slabs, stairs), Bridge (deck block, height over water, piers), Tunnel (cut with a roof past a set cover),
  Clear plants. Laid in arc-length steps, so the same road comes out the same in any chunk.
- **Junctions:** an end dropped on another road snaps to it; where two roads overlap, the higher priority's
  palette wins and the grade blends.
- **Canals:** the same component with a Water block and a depth.
- **Area Fill component:** an outline, a palette or pattern, Level or Follow, slope out.
- **Scripts:** `Road.Points`, `Roads.Nearest(point)`, distance along and side of a road.

### Placing and scattering (stage 3)

- **Placement component** on a prefab root: footprint, front and door (child Obj references). Kits write it.
- **Palette panel:** a grid of prefab thumbnails from chosen folders. Click places on the ground, R turns by a set
  step. With a Road selected, the prefab faces the road and sits a set distance back from its edge.
- **Line up along a road:** both sides or a stretch, from a weighted list, facing the road, set back with a little
  jitter, no two alike side by side, skipping places taken by other Placements. Deterministic from a seed, so
  "again with another seed" is one click.
- **Prefab brush:** weighted prefabs painted as real Objs on the ground or any mesh, with density, spacing, turn,
  size, lean with ground and steepest slope; Shift removes; under a chosen parent.
- **Scatter in an area:** fill an outline from a list, by density.
- **Clear plants** from stage 1 applies to grown foliage and generated trees.
- Every placement is one undo step a stroke (a group of Exists steps).

### Navigation for placed models (stage 3)

- A Mesh Collider marked **Walkable** is rasterised into a solid mask per block chunk it covers, from its world
  bounds, and block nav reads that mask with the blocks: floors and stairs to stand on, walls blocked, doorways
  joined to the ground outside. Moving the Obj dirties the chunks under its old and new bounds, like a layer.
- **Kits:** the mask is baked once per prefab mesh at each of four turns and stamped per instance.
- **Doors:** a Door component (or a hinge in a kit) marks its cells as a door that opens, which Nav Agents and
  Block Mobs path through.
- `navigation.status` counts walkable models; the Navigation gizmo draws their places.

### Painted maps (stage 4)

- A map is kept in 64x64 tiles, only where painted, saved with the world in `<world>.maps`, endless like the world.
- **Painted Map node:** reads a map by typed reference and outputs its value, with a default where unpainted. Its
  kernel holds the map by `Arc` and hashes the map's id and version, as Climate Mask does with regions.
- **Land, Height and Biome** are wired in without nodes: land before the coast curve, height before ridges,
  erosion and gullies, biome before the climate rules with a ragged border.
- **Brushes** in the Block World Inspector beside Build: Land, Height, Biome and Erase, with size, strength and
  falloff; Shift takes away, Ctrl smooths. Each stroke is one undo step.
- **While painting,** the far view's coarse columns under the brush are made again quickly; full chunks are made
  when the stroke ends. This needs the dirty path from stage 1 to cover the generator too: a painted change keys
  the far-tile cache by the tiles it touched, not the world hash.

### World Areas and Routes (stage 5)

- **World Area component:** outline or a painted area map, height range, priority, tags. A tag is a typed
  reference to a Tag asset; each entry carries a data asset. The world lists defaults per tag, and per biome.
- `Areas.At(point)` and `Areas.Value<T>(point, tag)`; tinted outlines with a toggle; an area brush.
- Structure placement gains Areas: within tagged, never within tagged. A No Structures tag stops the generator.
- **Route component:** points, kind (a Route Kind asset; road, sea and air come built in), width, one-way.
  A Road is a Route of kind road. **Route Stop component:** a place on a route with the game's own data.
- `Routes.Find(from, to, kind)`, `Routes.Nearest(point, kind)`, and a cursor by distance.
- **Sea route tool:** lays a route between two stops over water deeper than a draught and further than a set
  distance from the shore, searched over the generator's coarse ground (far tiles), so it works far beyond the
  made world. Routes that run aground after a repaint are drawn red.
- Routes draw in the Scene view and on the map.

### Editor

- A **World** panel in the rail's drawer with Paint, Road, Place, Scatter, Areas and Routes, and the layer kinds'
  visibility toggles.
- Every tool reachable from `vox editor call` and `vox mcp` (new `CallInfo` entries in the link; components go
  through `component.add` and `field.set`).
- Undo a stroke or a drag at a time, through the existing `field_drag_start` / `record_step` path.

### Limits from the Fantasy City (spec section 9)

- **A box model with one bone and no clips is drawn unskinned:** checked at import, the skin is dropped.
- **Static box models:** a Mesh Renderer of the box model's mesh with a Mesh Collider is written into the manual
  as the way to make one, and the importer gets a "Static" choice that does it.
- **Two renders at once:** to be looked into in stage 1 (likely both writing the same far-tile or pipeline cache).

## Open questions, answered

| Question | Recommendation |
|---|---|
| Ground Fit to whole blocks only, or slabs? | **Whole blocks for the footprint**, so floors are level with the blocks around them. On the slope-out ring, slabs where the world has slabs turned on, so the eased edge walks smoothly. One setting turns them off. |
| One spacing for painted maps, or finer for biomes? | **A spacing per map**, chosen when it is made: 8 m for Land and Height, 4 m for Biome. Biome borders are ragged anyway, so 4 m is fine enough. |
| A repaint moves the ground under a city: follow or stay and re-cut? | **Stay put and re-cut.** The Ground Fits re-shape the new ground, so the city stays as designed and nothing moves without being asked. Where a slope out can no longer reach the new ground, the gizmo draws the ring red. A "Settle on ground" command moves the selected Objs onto the new ground when wanted. |
| Is a World Area flat, or does it need a height range and stacking? | **A height range, open by default,** plus priority. That covers caves and dungeon floors stacked under a town at almost no cost, and a flat area is just the default. |
| How much of a route network is saved, and how much worked out at load? | **Saved:** what was authored: routes' points, kinds, widths, stops, and sea routes once laid (as points). **Worked out at load:** the graph, its joins and costs, and the "aground" check. **Asked on demand and cached, never saved:** the generator's own town roads. |

## Stages

Each stage lands on master with its tests, `vox render` pictures checked close up in a project copy, manual
pages, and `docs/ARCHITECTURE.md` and `docs/ROADMAP.md` brought up to date.

| Stage | Contents | Size |
|---|---|---|
| 1 | World layers (index, per-column shaping, local remake keeping hand edits, far view and map), Ground Fit with gizmo and snapping, column events and Streams With Ground, structure Ground gains Cut and slope out | Largest; the base for everything |
| 2 | Roads (points, widths, palette and Pattern, edge, Grade, steps, bridges, tunnels, canals, junctions), Area Fill | Large |
| 3 | Placement component, palette panel, line up, prefab brush, scatter, Walkable mesh nav with doors; box models without skin | Large |
| 4 | Painted maps, Painted Map node, Land, Height and Biome wired in, brushes, quick preview, local remake of generator changes | Large |
| 5 | World Areas with tags and defaults, structure placement by area, Routes and Stops, network queries, sea route tool, map drawing | Medium |
