---
order: 3
---

Something far away covers only a few pixels on screen, so drawing it in full detail is wasted work. Voxear has two ways to draw less of it: the simpler copies every model gets by itself, and **LOD Groups**, which swap one Obj for another as you set.

## A mesh's own levels of detail

Every model gets simpler copies of its meshes when it is imported, or uses the ones it comes with (see [[Models]]). The engine picks one for each Obj every frame, by how much detail would show at that size on screen, and fades between them so the change does not pop. There is nothing to set up.

Select an Obj with a **Mesh Renderer** to see its mesh's levels in the Inspector. The **LODs** row says which level the Scene view draws now, of how many, and how many triangles it has; while the game plays, it says what the game's camera draws. Rest the pointer on **LODs** to see every level's triangles, the most detailed first.

Fly the Scene view closer and further away to see the level change.

## LOD Groups

A **LOD Group** shows one of several Objs at a time, chosen by how big the group is on screen or by how far away it is. Use one when the far version should be something other than a simpler copy: a flat picture of a tree in place of the tree, a building without its inside, or a different object altogether at a distance.

To make one:

1. Make an Obj to hold the group, and put the Obj for each level inside it, the most detailed first. Each level's Obj is shown with everything inside it.
2. Select the holding Obj, click **Add Component** and choose **LOD Group**. Each Obj inside it becomes a level, in the order the Hierarchy shows them.

The Inspector shows the group's levels as a bar, the most detailed on the left, with a line where the Scene view sees the group now. Under the bar it says how much of the screen the group covers from there, how far away it is, and which level it shows. Each level has a row:

| Setting | What it does |
| --- | --- |
| **Switch by** | **Screen size** switches by how much of the screen's height the group covers, as Unity does. **Distance** switches by how far the camera is from the group's Obj, in metres. |
| **LOD 0**, **LOD 1** and on | The Obj the level shows. Pick another from the Objs inside the group, or **None** to show nothing at that level. |
| **Down to** | Measured by screen size: the share of the screen, in percent, the group can shrink to and still show this level. Smaller than that, the next level shows. |
| **Out to** | Measured by distance: how far away, in metres, this level shows. Further than that, the next level shows. |
| **Culled** | Whether nothing shows once the group is past its last level. Turned off, the last level stays. |

**Add Level** adds a level for the first Obj inside the group that no level shows yet, and **Remove Last Level** takes the last one away.

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
levels = [{ obj = 2, until = 30.0 }, { obj = 3, until = 120.0 }]
```

Measured by screen size, `until` is a share of the screen from 0 to 1, so `0.25` is a quarter of it.
