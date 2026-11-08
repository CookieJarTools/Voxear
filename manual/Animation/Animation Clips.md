---
order: 1
---

An animation clip is part of the model file that holds it, as its meshes and materials are. Open a model out in the Project panel and its clips are listed under it, each with a play mark. Drag one onto a clip field, or pick it from the field's list. A field shows the clip with its file, as `Walk_Loop · Walks`, and the scene keeps it as `Models/Walks.glb#clip:Walk_Loop`.

Clips come from glTF and FBX files. A file needs no mesh to hold clips: an animation pack of nothing but clips works, and its clips play on any character with the same rig, or on any humanoid.

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

## Humanoids named otherwise

A clip made for one humanoid plays on another whose rig names its bones otherwise, as `DEF-thigh.L` against `thigh_l`. When the clip has no bones named as the character's hips, head and hands, both rigs are read as humanoids instead:

- the hips, the spine, the neck and the head;
- each arm, from the collarbone down to the first three bones of each finger;
- each leg, from the thigh down to the toes.

Each of the character's bones then turns as its part turns in the clip. A rig whose bones stand otherwise at rest, or whose spine has more or fewer bones, plays the clip all the same. The hips travel as far as the character's legs are long.

The parts are found by the names most rigs give them:

| Part | Found by |
| --- | --- |
| Hips | `hips` or `pelvis` |
| Head | `head`, with `neck` above it when the rig has one |
| Collarbone | `clavicle` or `shoulder` |
| Arm | `hand`, and the two bones above it, passing over twist bones |
| Fingers | `thumb`, `index`, `middle`, `ring`, and `pinky` or `little` |
| Leg | `foot`, and the two bones above it |
| Toes | `toe` or `ball` |

Each side's bones carry their side in the name, as `Left`, `.L`, `_l` or `L_`. Any other bone, such as a cape, a tail or a gun's carrier, plays when both rigs name it alike, and otherwise stays at rest.

## Where clips play

- An [[Animator]] plays one clip.
- A [[Motion Sets|Motion Set]] gathers the clips a character walks, crouches and jumps with, and its [[Actions]].
