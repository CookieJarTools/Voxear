---
order: 1
---

Voxear reads models in two formats:

- **glTF**, the open format most modelling programs can export: either a `.gltf` file, or a single `.glb` file with everything packed inside. In Blender, use **File › Export › glTF 2.0**.
- **FBX** (`.fbx`), the format many game tools and asset stores use. In Blender, use **File › Export › FBX**.

If you can choose, pick glTF: it says more about how materials should look. Both work the same way once they are in your project.

## Adding a model to your project

Drag the model file from your file manager onto the Project panel, with any pictures it uses. The files are copied into the folder you let go on: a folder's tile, a folder in the tree, or the space around the files, which means the folder the Project panel shows. A `Models` folder is a good place. If a file of that name is already there, the copy is numbered, as in `Ship 1.glb`.

You can also right-click a folder, or the space around the files, and choose **Import New Asset…** to pick the files in your desktop's file chooser. They are copied into that folder. On Linux this uses zenity, which most desktops have, or KDE's kdialog.

On Linux, some desktops cannot pass a drag from an X11 program to the editor. If a drop does nothing, open your file manager on its own, not from another program such as the Unity editor, and drag from there, or use **Import New Asset…**.

You can also copy files into the project's folder yourself. The editor sees them when you come back to it. In the same way, when you export a model again over its file, Voxear imports it again when you come back to the editor, and everything drawing it shows the change.

Select the model in the Project panel to see it in the Inspector. Drag the preview to turn it.

When a model is first used, Voxear makes simpler copies of it, called **levels of detail** (LODs), for when it is far away and small on screen. You do not need to set anything up, and there is no component to add: the engine picks the right one as the camera moves, and blends smoothly between them. To leave them out, turn off **Make LODs** in the model's import settings. The Inspector shows them on the Obj's Mesh Renderer, and a **LOD Group** swaps whole Objs as they get further away; see [[Levels of Detail]].

A model that comes with LODs of its own uses those instead. Name each mesh's versions as Unity reads them: `Rock_LOD0` for the full one, then `Rock_LOD1`, `Rock_LOD2` and so on, each simpler than the last. The model then has one part, `Rock`, drawn with your LODs at the distances the engine's own would be used. To have the engine make them from `Rock_LOD0` instead, turn off **Use its own LODs**. Either way, the later LODs are not parts of their own.

A model made for Unreal, as many from asset stores are, often carries collision meshes beside its LODs, named `UCX_`, `UBX_`, `USP_` or `UCP_` before the mesh's name. Voxear leaves them out, so they never cover the model in boxes.

## Putting a model in a level

Drag the model from the Project panel into the Scene view. As in Unity, it shows there as soon as the pointer is over the view, following it so you see where it will go, and lands where you let go: where the pointer meets the ground, or on top of whatever is under the pointer. Drag it off the view to change your mind. You can also drag it onto the Hierarchy: let go on an Obj to put the model inside that Obj, or on a scene's name to put it in that scene. Press Ctrl+Z to take it away again.

The new Obj is named after the model. A model of one part gets a **Mesh Renderer** that draws it with its own material. A model of several parts, such as a whole building, stays one Obj too: its **Model Renderer** draws every part where the model puts it, each with the model's material for it, so the Hierarchy shows one Obj however many parts it has. Each part is still culled, given its LOD and shadowed on its own, and clicking any part in the Scene view selects the Obj.

The Model Renderer shows the **Model** it draws, its **LOD bias** (as a Mesh Renderer's) and how many **Parts** it has. To move, hide or change one part on its own, open the Model Renderer's menu (the **⋯** on its header, or a right-click) and pick **Unpack to Objs**: the Obj then has an Obj inside it for each part, each with a Mesh Renderer, where the model puts it. Ctrl+Z packs it back up.

In a scene file:

```toml
[obj.model]
model = "fdfa941753cd46a487317019a970cbc2"  # Models/Sponza/Sponza.gltf
```

If you will use the model many times with the same settings, make it a prefab and place that instead. See [[Prefabs]].

## A model's parts

A model can hold several meshes, the shapes it is made of, and the materials they are drawn with. Click the arrow on the model's tile in the Project panel to open it out: its meshes, then its materials, show after it. Click the arrow again to close it.

Drag a mesh onto a Mesh Renderer's **Mesh** in the Inspector to draw that mesh, or a material onto its **Material** to draw with that material. You can also click the arrow on **Mesh** and type to search: it lists the built-in shapes, then every mesh of your models, each by the name its model gives it and its part, as `Gate #mesh0.3`, with the model's file beside it. A model the editor has not loaded yet is listed whole, by its file; picking it uses its first mesh.

## A model's materials

Select a model in the Project panel, and the Inspector lists its **Materials** by the names the file gives them, as Unity's model importer does. Beside each is what it is drawn with: the model's own material, shown greyed by its name, or a material file picked in its place. Pick a material file from the field's arrow, or drag one onto it, and everything drawing the model shows it at once.

A model's own materials also show inside it in the Project panel, by name, each with a preview, after its meshes: open the model with the arrow on its tile. They cannot be changed, as they live inside its file. To edit them, click **Extract Materials**. Voxear writes each one out as a material file, in a folder beside the model named after it, such as `Models/Gate Materials`, and draws the model with them. Pictures packed inside the model's file are written out beside them as `.png` files; pictures kept beside the model stay where they are. Then select a material in that folder to change it. See [[Materials]].

The choices are kept in the model's `.meta` file, each material by its part, with the material file's ID:

```toml
[materials]
material0 = "8d2f6a1c3e5b47d9a0c2e4f6b8d0a1c3"
```

A build ships the material files a model is drawn with, and their pictures.

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
- `#mesh0.0` is the first part of the first mesh in the model; `#mesh1.0` the first part of the second, and so on. A Mesh Renderer draws one part; a Model Renderer draws them all.
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
| **Use its own LODs** | For a model whose meshes come with LODs of their own, named `_LOD0`, `_LOD1` and on: whether it is drawn with those. Turned off, the engine makes them from the first, as **Make LODs** says. |
| **Make LODs** | Whether the simpler copies for far away are made. Turn it off for a model that must always be drawn in full. |
| **Import materials** | Whether the model is drawn with its own materials. Turned off, each part is drawn plain grey, and fields that name one of its materials still work. A material listed under **Materials** with a material file of its own is drawn with that either way. |
| **Wind masks** | For plants: what says how each part moves in the wind (see [[Wind]]). **None** bends the whole plant as one. **From vertex colours** reads masks painted in the model's vertex colours. **Generate** works them out from the plant's shape. |

Everything that draws the model shows the change at once. A part of the model placed in an open scene moves to where the model now puts it, unless you have moved it since. A Model Renderer always draws the parts where the model now puts them. In scenes that are not open, the parts stay where they were.

The settings are kept in the model's `.meta` file, beside its ID, so keep the two files together when you move or copy the model. Only settings you changed are written:

```toml
guid = "277e6fd45f82419a894b65b6b72ed398"

[model]
units = "centimetres"
scale = 2.0
```

A build uses the same settings, and builds the model again when they change.

## A picture's import settings

Select a picture (`.png` or `.jpg`) in the Project panel, and the Inspector shows its size in pixels and its **Import Settings**. They apply wherever a model reads the picture, as a colour, a normal map or glow. Click **Apply**, and every model in use that reads the picture is imported again.

| Setting | What it does |
| --- | --- |
| **Max size** | The most pixels the picture's longer side keeps. A larger picture is shrunk, keeping its shape. **Its own size** keeps every pixel. |
| **Compress in builds** | Whether a build ships the picture compressed, in a quarter of the memory. Turn it off for pixel art and sharp edges, which compression softens. The editor never compresses pictures. |

The settings are kept in the picture's `.meta` file, as a model's are:

```toml
[texture]
max_size = 1024
compress = false
```

A picture packed inside a model file has no `.meta` of its own, so it is always read at its own size and compressed in builds.

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
