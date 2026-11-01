---
order: 2
---

A material says what a surface looks like. Every Mesh Renderer draws its shape with one.

## Making a material

Right-click in the Project panel and choose **New Material**, then type its name. Select it, and the Inspector shows what it holds:

| Setting | What it does |
| --- | --- |
| Colour | The surface's own colour |
| Emission | Light the surface gives off, so it glows. Black for none |
| Metallic | 0 for plastic, wood or stone; 1 for bare metal |
| Roughness | 0 for a smooth, mirror-like shine; 1 for a dull, matt surface |

Every change is saved straight away, and shows at once on everything that uses the material.

Click a colour's swatch to open the colour picker: drag in the square and the bar, or type a hex code such as `#FF8800`, or red, green and blue from 0 to 1.

## Using a material

Select an Obj with a Mesh Renderer, and click its **Material** field in the Inspector. Pick the material from the list, or drag it there from the Project panel. **Default** is the plain grey every new shape starts with.

## Making a material glow

Give a material an **Emission** colour to make it glow. Emission's colour picker has an extra **Intensity** slider: each step up doubles the brightness. Bright emission spills a soft glow around it on screen, called **bloom**.

A glowing surface does not light up what is around it. To make a lamp that lights the room, put a Point Light beside its bulb. See [[Lights]].

## The material file

A material is a small text file:

```toml
color = [0.8, 0.6, 0.2]
metallic = 1.0
roughness = 0.3
emissive = [0.0, 0.0, 0.0]
```

- `color` is red, green and blue from 0 to 1, as a colour picker shows them.
- `emissive` can go above 1 for a brighter glow.

A material file holds no pictures yet. For a surface with a texture, a normal map or a cut-out, use the material that came with a model, as `Models/Crate.gltf#material0`. See [[Models]].

## Materials written into a scene

A scene file can also give a Mesh Renderer its own material, written right there, with the same settings:

```toml
[obj.renderer]
mesh = "builtin:sphere"
material = { color = [0.55, 0.36, 0.96], roughness = 0.35 }
```

The Inspector shows these as **Custom**. They cannot be changed in the Inspector yet: pick a material file in their place to change them in the editor.
