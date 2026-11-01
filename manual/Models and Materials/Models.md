---
order: 1
---

Voxear reads models in two formats:

- **glTF**, the open format most modelling programs can export: either a `.gltf` file, or a single `.glb` file with everything packed inside. In Blender, use **File › Export › glTF 2.0**.
- **FBX** (`.fbx`), the format many game tools and asset stores use. In Blender, use **File › Export › FBX**.

If you can choose, pick glTF: it says more about how materials should look. Both work the same way once they are in your project.

## Adding a model to your project

Copy the model file into your project's folder, with any pictures it uses beside it. A `Models` folder is a good place.

The editor finds new files when the project opens. A file copied in while the editor is open shows up after you next make, rename, duplicate or delete something in the Project panel, or when you open the project again.

Select the model in the Project panel to see it in the Inspector. Drag the preview to turn it.

When a model is first used, Voxear makes simpler copies of it, called **levels of detail**, for when it is far away and small on screen. You do not need to set anything up: the engine picks the right one as the camera moves, and blends smoothly between them. To leave them out, turn off **Make LODs** in the model's import settings.

## Putting a model in a level

Drag the model from the Project panel into the Scene view. It lands where the pointer meets the ground, or on top of whatever is under the pointer. You can also drag it onto the Hierarchy: let go on an Obj to put the model inside that Obj, or on a scene's name to put it in that scene. Press Ctrl+Z to take it away again.

The new Obj is named after the model, and has a **Mesh Renderer** that draws the model with its own material. A model made of several parts becomes an Obj with an Obj inside it for each part.

If you will use the model many times with the same settings, make it a prefab and place that instead. See [[Prefabs]].

## A model's parts

A model can hold several meshes, the shapes it is made of, and the materials they are drawn with. Click the arrow on the model's tile in the Project panel to open it out: its meshes, then its materials, show after it. Click the arrow again to close it.

Drag a mesh onto a Mesh Renderer's **Mesh** in the Inspector to draw that mesh, or a material onto its **Material** to draw with that material.

### A model in the scene file

In the scene file, the Obj looks like this. You can also write it by hand:

```toml
[[obj]]
id = 20
name = "Pyramid"
position = [0.0, 0.0, 4.0]
[obj.renderer]
mesh = "Models/Pyramid.gltf#mesh0.0"
material = "Models/Pyramid.gltf#material0"
```

- `id` must be a number no other Obj in the file uses.
- `#mesh0.0` is the first part of the first mesh in the model; `#mesh1.0` the first part of the second, and so on. A model made of several parts needs an Obj for each.
- `#material0` is the model's first material. Leave `material` out and the model is drawn plain grey.

If you write it by hand, save the file and open the level again. When the editor saves the scene, it writes the model's ID (from its `.meta` file) in place of its path, so the link survives the model being renamed.

## A model's import settings

Select a model in the Project panel, and the Inspector shows its **Import Settings**, as Unity's model importer does. Change them, then click **Apply** to import the model again with them. **Revert** puts back the settings last applied.

| Setting | What it does |
| --- | --- |
| **Units** | The length one unit in the file stands for. **As the file says** trusts the file: glTF is always in metres, and an FBX file names its own unit. Pick another when a model comes in at the wrong size, such as one made in centimetres whose file says metres. |
| **Scale** | Makes the model bigger or smaller, after its units. 2 is twice the size. |
| **Normals** | Which way each surface faces, for lighting. **From the file** uses the file's, and works them out for a mesh that has none. **Calculate** always works them out. |
| **Smoothing angle** | For calculated normals: an edge whose faces meet at less than this many degrees is smooth, and a sharper one stays hard. |
| **Tangents** | What normal maps need. **From the file** uses the file's, and works them out for a mesh that has none. **Calculate** always works them out. They are worked out whenever the normals are. |
| **Make LODs** | Whether the simpler copies for far away are made. Turn it off for a model that must always be drawn in full. |
| **Import materials** | Whether the model is drawn with its own materials. Turned off, each part is drawn plain grey, and fields that name one of its materials still work. |

Everything that draws the model shows the change at once. A part of the model placed in an open scene moves to where the model now puts it, unless you have moved it since. In scenes that are not open, the parts stay where they were.

The settings are kept in the model's `.meta` file, beside its ID, so keep the two files together when you move or copy the model. Only settings you changed are written:

```toml
guid = "277e6fd45f82419a894b65b6b72ed398"

[model]
units = "centimetres"
scale = 2.0
```

A build uses the same settings, and builds the model again when they change.

## What is read from a glTF file

- The shape, its normals (which way each surface faces) and its texture coordinates. If the file has no normals, Voxear works them out.
- Its materials: colour, metal and roughness, normal map, and glow, each with its picture.
- Cut-outs, such as leaves on a flat card, where the material says to cut them out.

Not yet read: animation, skinning, vertex colours, see-through materials (they are drawn solid), and the cameras and lights in the file.

## What is read from an FBX file

- The shape, its normals and its texture coordinates. If the file has no normals, Voxear works them out.
- Its size and which way is up. A model made in centimetres comes in at the right size, and a model from a program where Z is up comes in standing up. Its Objs show no extra turn to undo.
- Its materials: colour, metal, roughness and glow, and the pictures for colour, normal map and glow. Older FBX materials (Phong and Lambert) work too. A colour picture takes the place of the colour.
- A mesh with several materials becomes one part for each material.

Pictures can be packed inside the FBX file, or kept beside it. FBX files often name a picture by a full path on the computer that made them, such as `C:\Art\wood.png`. Voxear looks for a picture with that name in the model's folder, so keep the pictures beside the model. A picture Voxear cannot find, or cannot read, is left out, and the model loads without it. Voxear reads PNG, JPEG and GIF pictures.

Not yet read: animation, skinning, vertex colours, see-through materials, metal and roughness pictures, and the cameras and lights in the file.

## The built-in shapes

Every project has three shapes ready to use, which the Hierarchy's **+** button makes:

| Shape | Size |
| --- | --- |
| Cube | 1 metre on each side |
| Sphere | 1 metre across |
| Plane | 1 metre square, facing up |

In a scene file they are `builtin:cube`, `builtin:sphere` and `builtin:plane`.

Next: [[Materials]].
