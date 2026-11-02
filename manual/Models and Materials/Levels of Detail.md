---
order: 3
---

Something far away covers only a few pixels on screen, so drawing it in full detail is wasted work. Voxear has two ways to draw less of it: the simpler copies every model gets by itself, and **LOD Groups**, which swap one Obj for another as you set.

## A mesh's own levels of detail

Every model gets simpler copies of its meshes when it is imported, or uses the ones it comes with (see [[Models]]). The engine picks one for each Obj every frame, by how much detail would show at that size on screen, and fades between them so the change does not pop. There is nothing to set up.

Select an Obj with a **Mesh Renderer** to see its mesh's levels in the Inspector. The **LODs** row has a cell for each level, with its triangles, most detailed first. The level the Scene view draws now is outlined; while the game plays, it is the one the game's camera draws.

- **Click** a cell to move the Scene view to where that level is drawn, looking at the Obj.
- **Right-click** a cell to **Force** that level, which is then drawn at every distance, as Unity's Forced LOD. **Stop Forcing** goes back to picking by distance.
- **LOD Bias**, above the cells, changes when the levels switch, as Unity's LOD Selection Bias does. At 1, simpler levels come sooner; at -1, later. 0 leaves it to the engine.

A scene file writes these on the Obj's `[obj.renderer]` as `lod_bias = 1.0` and `force_lod = 2`.

## LOD Groups

A **LOD Group** shows one of several Objs at a time, chosen by how big the group is on screen or by how far away it is. Use one when the far version should be something other than a simpler copy: a flat picture of a tree in place of the tree, a building without its inside, or a different object altogether at a distance.

To make one:

1. Make an Obj to hold the group, and put the Obj for each level inside it, the most detailed first. Each Obj a level shows is shown with everything inside it.
2. Select the holding Obj, click **Add Component** and choose **LOD Group**. Each Obj inside it becomes a level, in the order the Hierarchy shows them.

The Inspector shows the group's levels as a bar, as Unity does, the most detailed on the left. The camera mark above it shows where the Scene view sees the group now: how much of the screen it covers, or how far away it is.

- **Click** a level on the bar to select it. Its settings show under the bar.
- **Drag** the edge between two levels to move where one ends.
- **Right-click** a level to **Insert Before** or **Delete** it, or the **Culled** part to **Insert** a level at the end. A new level shows the first Obj inside the group that no level shows yet.

| Setting | What it does |
| --- | --- |
| **Switch by** | **Screen size** switches by how much of the screen's height the group covers, as Unity does. **Distance** switches by how far the camera is from the group's Obj. |
| **Objs** | The Objs the selected level shows, as Unity's LOD lists its renderers. Pick one into **Add Obj** to add it, from the Objs inside the group, and pick **None** on one to take it out. |
| **Transition** | Where the selected level ends: a share of the screen in percent, or a distance in metres. Past it, the next level shows. |
| **Culled** | Whether nothing shows past the last level. Turned off, the last level stays. |

Each view chooses for itself, so a Scene view close up and the Game view far away can show different levels of the same group at once. Only drawing changes: the Objs of a level not shown are not drawn and their lights do not shine, but their scripts keep running.

A LOD Group can be part of a prefab. Each copy of the prefab switches between its own Objs.

## In a scene file

A scene or prefab file writes a LOD Group under its Obj as `[obj.lod_group]`, each level naming its Obj by id:

```toml
[[obj]]
id = 1
name = "Tree"
[obj.lod_group]
measure = "distance"   # or "screen_size", as when left out
cull = false           # the last level stays past its end
levels = [{ objs = [2, 5], until = 30.0 }, { objs = [3], until = 120.0 }]
```

Measured by screen size, `until` is a share of the screen from 0 to 1, so `0.25` is a quarter of it.
