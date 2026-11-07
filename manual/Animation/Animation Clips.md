---
order: 1
---

An animation clip is part of the model file that holds it, as its meshes and materials are. Open a model out in the Project panel and its clips are listed under it, each with a play mark. Drag one onto a clip field, or pick it from the field's list. A field shows the clip with its file, as `Walk_Loop · Walks`, and the scene keeps it as `Models/Walks.glb#clip:Walk_Loop`.

Clips come from glTF and FBX files. A file needs no mesh to hold clips: an animation pack of nothing but clips works, and its clips play on any character with the same rig.

## Clip settings

Select a model file and its import settings in the Inspector have a **Clips** section, a row for each clip:

- **Motion**: how the clip moves the character.
  - **Auto**: by its root bone when the root travels, else by its feet, for a loop made to stay in place.
  - **From Root**: by its root bone.
  - **From Feet**: by how fast its planted foot slides back.
  - **Set**: at the **Direction** and **Speed** given, for a clip that stays in place: Direction in degrees, 0 ahead, 90 to the right, 180 back, and Speed in metres a second. With Root Motion on, a Set clip moves the character at exactly that.
- **Loop**: **Auto** loops a clip whose last frame is its first; **On** and **Off** say so whatever the clip.

Press **Apply** to keep them. They are kept in the model's `.meta` file.

## The same rig

A clip plays on a character other than the one it was made with when the two share a rig: bones of the same names, standing the same way at rest. Bone by bone, by name, the clip turns each bone as it was made to, and the character keeps its own bone lengths. The hips go up and down as far as the character's legs are long. A bone the clip does not have stays at rest.

Rigs with other bone names do not play each other's clips yet.

## Where clips play

- An [[Animator]] plays one clip.
- A [[Motion Sets|Motion Set]] gathers the clips a character walks, crouches and jumps with, and its [[Actions]].
