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
| Colour Map | A picture of the surface's colour, tinted by **Colour** |
| Metallic Map | A picture of where it is metal, in blue, and how rough, in green, as glTF has them |
| Normal Map | A picture of the surface's bumps and dents |
| Emission Map | A picture of where it glows, tinted by **Emission** |

Every change is saved straight away, and shows at once on everything that uses the material.

To give a map a picture, click the arrow on its field and pick one, or drag a picture from the Project panel onto the field. Pick **None** to take it away. Click the field to find the picture in the Project panel.

Click a colour's swatch to open the colour picker: drag in the square and the bar, or type a hex code such as `#FF8800`, or red, green and blue from 0 to 1.

## Using a material

Select an Obj with a Mesh Renderer, and click its **Material** field in the Inspector. Pick the material from the list, or drag it there from the Project panel. **Default** is the plain grey every new shape starts with.

You can also drag a material from the Project panel straight onto an Obj in the Scene view. While you hold it over an Obj, the Obj shows the material, so you can try it on a few before choosing; move away and the Obj looks as it did. Let go to keep it. Dropping a material on an Obj's row in the Hierarchy works too. **Ctrl+Z** undoes either.

## Making a material glow

Give a material an **Emission** colour to make it glow. Emission's colour picker has an extra **Intensity** slider: each step up doubles the brightness. Bright emission spills a soft glow around it on screen, called **bloom**.

A glowing surface does not light up what is around it. To make a lamp that lights the room, put a Point Light beside its bulb. See [[Lights]].

## Leaves and grass

Turn on **Foliage** for leaves, grass and anything else thin that light shines through. Foliage takes the sun differently:

- Light wraps a little way round to the side turned from the sun, so leaves never go flat and dark.
- Seen against the sun, the leaf glows in its own colour, as sunlight through a leaf does.
- Edges facing away from the camera catch a rim of the sun's colour.

Only the sun lights foliage this way. A leaf's shaded side takes the light of its sunlit side, so a leaf in a canopy's shade stays in shade, but a leaf on its own lights up from behind.

## The material file

A material is a small text file:

```toml
color = [1.0, 1.0, 1.0]
metallic = 0.0
roughness = 0.8
emissive = [0.0, 0.0, 0.0]
color_map = "5b0e2a7c41d94f3c8a1f0e6b2d9c7a13"
normal_map = "c81f3e0a9b2d4c6e8f1a3b5d7e9f0a2c"
```

- `color` is red, green and blue from 0 to 1, as a colour picker shows them.
- `emissive` can go above 1 for a brighter glow.
- `foliage = true` lights it as leaves are, as above.
- `color_map`, `metallic_map`, `normal_map` and `emissive_map` name pictures by their IDs, from their `.meta` files, so the link survives a picture being renamed.

A model's own materials, with their pictures, can be written out as material files to edit. See [[Models]].

## Materials written into a scene

A scene file can also give a Mesh Renderer its own material, written right there, with the same settings:

```toml
[obj.renderer]
mesh = "builtin:sphere"
material = { color = [0.55, 0.36, 0.96], roughness = 0.35 }
```

The Inspector shows these as **Custom**. They cannot be changed in the Inspector yet: pick a material file in their place to change them in the editor.
