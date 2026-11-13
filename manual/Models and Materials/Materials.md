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
| Height Map | A picture of how high each point of the surface stands, which **Displacement** raises it by. See [[Displacement]] |
| Sharp pixels | Draws the colour map's pixels sharp however near, each a square of one colour, as pixel art is |
| Connected tiles | Its maps are five tiles side by side, and a block's faces drawn with it join those of its kind beside them. See [Pictures for blocks](#pictures-for-blocks) |
| Ground blend | How many metres up from where a model meets a terrain it takes on the ground's own look, so rocks and walls sit in the ground. 0 for not at all. See [[Blending Models into Terrain]] |

Every change is saved straight away, and shows at once on everything that uses the material.

To give a map a picture, click the arrow on its field and pick one, or drag a picture from the Project panel onto the field. Pick **None** to take it away. Click the field to find the picture in the Project panel.

Click a colour's swatch to open the colour picker: drag in the square and the bar, or type a hex code such as `#FF8800`, or red, green and blue from 0 to 1.

## Using a material

Select an Obj with a Mesh Renderer, and click its **Material** field in the Inspector. Pick the material from the list, or drag it there from the Project panel. **Default** is the plain grey every new shape starts with.

You can also drag a material from the Project panel straight onto an Obj in the Scene view. While you hold it over an Obj, the Obj shows the material, so you can try it on a few before choosing; move away and the Obj looks as it did. Let go to keep it. Dropping a material on an Obj's row in the Hierarchy works too. **Ctrl+Z** undoes either.

## Making a material glow

Give a material an **Emission** colour to make it glow. Emission's colour picker has an extra **Intensity** slider: each step up doubles the brightness. Bright emission spills a soft glow around it on screen, called **bloom**.

**Glow** makes the emission move: **Pulse** rises and falls evenly, as a beacon; **Breathe** swells slowly and rests dim between, as a glowing mushroom; **Flicker** wavers unsteadily, as a flame or a firefly. **Glow speed** is how many times a second it goes round. Each Obj, and each plant of a Foliage, glows at its own moment, so a field of them never pulses as one, and the light they bounce onto what is round them rises and falls with them. In a material file, `glow = "breathe"` and `glow_speed = 0.3`.

A glowing surface does not light up what is around it. To make a lamp that lights the room, put a Point Light beside its bulb. See [[Lights]].

## Leaves and grass

Turn on **Foliage** for leaves, grass and anything else thin that light shines through. Foliage takes the sun differently:

- Light wraps a little way round to the side turned from the sun, so leaves never go flat and dark.
- Seen against the sun, the leaf glows in its own colour, as sunlight through a leaf does.
- Edges facing away from the camera catch a rim of the sun's colour.

Only the sun lights foliage this way. A leaf's shaded side takes the light of its sunlit side, so a leaf in a canopy's shade stays in shade, but a leaf on its own lights up from behind.

In a level with [[Seasons]], **Leaves** says what foliage does through the year: **Turn** takes each season's colour, and **Stay** stays as made, as a pine does. In a material file, `leaves = "stay"`.

## Glass

Turn on **Glass** for windows, bottles, lenses and anything else you see through. What lies behind glass shows through it:

- The **Colour** tints it: white is clear, green is bottle glass.
- **Roughness** frosts it: near 0 is clear, 0.5 and up blurs what lies behind to soft shapes, as frosted glass does.
- Its surroundings and the lights reflect in it, faintly head on and strongly at a grazing angle, as real glass does.
- A normal map ripples what you see through it, as old window glass does.

Light passes through glass, taking its colour: sunlight through green glass falls green on the floor behind it, and through frosted glass it spreads into a soft patch, wider the farther behind the glass it falls. Glass casts no dark shadow, so a room behind a window is lit almost as if the window were open. You see glass through other glass, as a bottle behind a window.

Every project has a clear glass material built in: pick **Glass** in a Mesh Renderer's Material menu, or write `material = "builtin:glass"` in a scene.

**IOR** (index of refraction) sets how strongly glass bends what lies behind it and how much it reflects: 1.5 suits window glass, 1.33 water and 2.4 diamond.

> [!NOTE]
> Glass in front of other glass does not show the glass behind it. Make each mesh's glass face outwards: you see glass only from the side its faces point to.

## Pictures for blocks

A [[Blocks|block]]'s faces are drawn with materials, its picture once on each face. Two settings are for them:

- **Sharp pixels** keeps a small picture crisp: a block sixteen pixels a side is sixteen squares a side from any distance, where a smoothed picture would blur up close. Far off, its pixels are still averaged, so the ground does not shimmer. It works on models too.
- **Connected tiles** reads the material's maps as five tiles side by side, and the block's faces join those of its kind beside them, as panes of glass do. See [[Blocks#Faces that join]]. Only a Block World reads the tiles so: on a model, the picture is drawn whole.

Whoever plays the game can swap a material's pictures with a [[Texture Packs|texture pack]]. The material's other settings stay.

## Custom shaders

Click the arrow on a material's **Shader** field to draw it with another shader in place of **Standard**: one built in, such as Water, Lava or Toon, or a short file of your own that changes its colour, how light falls on it, or moves its vertices. Its properties show as fields under the material's settings. See [[Custom Shaders]].

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
- `cutout = 0.5` cuts the surface out where its colour map's alpha is under 0.5, as leaves and grass cards are cut from their pictures. Leave it out for solid surfaces, which draw faster.
- `pixels = true` draws its colour map's pixels sharp, and `connected = true` reads its maps as the five tiles of a block's face that joins.
- `ground_blend = 0.4` blends a model into the terrain it stands on, 0.4 m up from the ground.
- `glass = true` makes it glass, as above, and `ior` sets its index of refraction, 1.5 unless written.
- `color_map`, `metallic_map`, `normal_map` and `emissive_map` name pictures by their IDs, from their `.meta` files, so the link survives a picture being renamed.
- `shader` names a custom shader by its ID, and `[properties]` holds the values the material gives it. See [[Custom Shaders]].

A model's own materials, with their pictures, can be written out as material files to edit. See [[Models]].

## Materials written into a scene

A scene file can also give a Mesh Renderer its own material, written right there, with the same settings:

```toml
[obj.renderer]
mesh = "builtin:sphere"
material = { color = [0.55, 0.36, 0.96], roughness = 0.35 }
```

The Inspector shows these as **Custom**. They cannot be changed in the Inspector yet: pick a material file in their place to change them in the editor.
