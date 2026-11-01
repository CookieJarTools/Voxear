---
order: 1
---

# Models

Voxear reads models in **glTF**, the open format most modelling programs can export: either a `.gltf` file, or a single `.glb` file with everything packed inside. In Blender, use **File › Export › glTF 2.0**.

## Adding a model to your project

Copy the model file into your project's folder, with any pictures it uses beside it. A `Models` folder is a good place.

The editor finds new files when the project opens. A file copied in while the editor is open shows up after you next make, rename, duplicate or delete something in the Project panel, or when you open the project again.

Select the model in the Project panel to see it in the Inspector. Drag the preview to turn it.

When a model is first used, Voxear makes simpler copies of it, called **levels of detail**, for when it is far away and small on screen. You do not need to set anything up: the engine picks the right one as the camera moves, and blends smoothly between them.

## Putting a model in a level

> [!NOTE]
> The editor cannot place a model in a level by dragging yet. For now, add it to the scene file by hand, as below. Once one Obj shows the model, you can duplicate it in the editor with Ctrl+D, or make it a prefab and place that.

Open the scene file in a text editor and add an Obj with a Mesh Renderer that names the model's mesh and material:

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

Save the file and open the level again. When the editor next saves the scene, it writes the model's ID (from its `.meta` file) in place of its path, so the link survives the model being renamed.

## What is read from a glTF file

- The shape, its normals (which way each surface faces) and its texture coordinates.
- Its materials: colour, metal and roughness, normal map, and glow, each with its picture.
- Cut-outs, such as leaves on a flat card, where the material says to cut them out.

Not yet read: animation, skinning, vertex colours, see-through materials (they are drawn solid), and the cameras and lights in the file. Export normals from your modelling program, since Voxear does not work them out itself.

## The built-in shapes

Every project has three shapes ready to use, which the Hierarchy's **+** button makes:

| Shape | Size |
| --- | --- |
| Cube | 1 metre on each side |
| Sphere | 1 metre across |
| Plane | 1 metre square, facing up |

In a scene file they are `builtin:cube`, `builtin:sphere` and `builtin:plane`.

Next: [[Materials]].
