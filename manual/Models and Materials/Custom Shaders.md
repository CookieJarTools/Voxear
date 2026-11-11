---
order: 7
---

A material draws its surface with the **Standard** shader unless you give it another. A custom shader is a short file in the Slang shading language that changes how a surface looks: its colour and shine, how light falls on it, or where its vertices are. Materials pick a custom shader as their **Shader**, and set its properties in the Inspector like any other setting.

The engine has shaders of its own built in, for water, lava, toon shading and more, and you can write your own.

## Built-in shaders

Every project has these. Click the arrow on a material's **Shader** field and pick one from under **Standard**.

| Shader | What it draws |
| --- | --- |
| Toon | Light in flat steps, as a cartoon is painted, with a coloured shade, a crisp shine, a rim of light and an optional ink line round the edges |
| Storybook | Soft, painted light made for the Storybook lighting look: a warm glow where light turns to shade, golden highlights, light catching the edges from behind, and colour that wanders as brushstrokes do |
| Triplanar | Pictures laid on by where the surface is rather than by its UVs, for rocks, cliffs and blocked-out levels, with a second picture over what faces up, as grass or snow on rock |
| Water | A sea, a lake or a pond: waves, ripples, light through the tops of the waves, foam where they break, and with **Glass**, depth you can see into and foam along the shore |
| Hologram | A figure of light, bright at its edges, with lines scrolling up it and a flicker. With **Glass**, a see-through force field |
| Sway | Vertices swaying to and fro, more the farther from the Obj's foot, for flags, washing lines, signs and plants |
| Lava | Plates of dark crust drifting on molten rock that glows through the cracks, its colour following its temperature |

A built-in shader costs nothing until a material uses it.

To change one, make a copy of your own: right-click in the Project panel, choose **New Shader…**, and pick it. The copy is an ordinary `.shader` file in your project, named for its file, to change as you like.

### Water

For a whole sea, a lake or a river, use [[The Ocean]], a [[Pools and Dry Areas|Pool]] or a [[Streams|Stream]] instead: their waves are real ones the game can ask about, and things float on them. The Water shader is for water on a mesh of your own shape.

- The waves move the mesh's vertices, so draw water on a mesh with plenty of them. On a single flat quad, only the light shows the waves.
- Turn on the material's **Glass** to see into the water. What lies under it shows through, bluer and darker the deeper it is, and foam laps where it meets the shore. **Clarity** says what a metre of water lets through, and **Murk** how far you see into it. Set the material's **IOR** to 1.33, as water's is.
- Without **Glass**, water is opaque, drawn in its **Deep** colour.

### Lava

For a flow of lava along a path, or a lava lake, see [[Lava]].

- **Temperature** sets the colour of the glow as real molten rock's: 1000 kelvin is a dull red, 1300 orange and 1800 yellow.
- Lava's glow lights nothing round it. Put an orange Point Light just above it to light the rocks nearby.

## Making a shader

Right-click in the Project panel and choose **New Shader…**, then pick what it starts as: **Lit**, **Unlit** for one that ignores light, or one of the built-in shaders to change. **UI** starts a shader for UI elements' backgrounds instead, which materials can't use: see [[UI Shaders]]. A new lit shader starts with a tint, a gloss and a pattern, and a vertex and a light function to uncomment and try:

```slang
struct Tinted
{
    [Property, Color] float3 tint = float3(1.0, 1.0, 1.0);
    [Property, Range(0, 1), Tooltip("How glossy it is: 1 is a mirror")] float gloss = 0.5;
    [Property] Picture pattern;
}

void surface(Tinted shader, SurfaceInput input, inout SurfaceOutput surface)
{
    surface.color *= shader.tint * shader.pattern.sample(input.uv).rgb;
    surface.roughness = 1.0 - shader.gloss;
}
```

Select a shader in the Project panel to see what the Inspector makes of it: its properties, which functions it has, and any error, with the line it is on.

Save the file and the editor compiles it into the engine's own shaders in the background. Everything keeps drawing as it was until the new shader is ready, a few seconds later; then every material using it changes at once. A shader with an error leaves the last one that worked drawing, and the error shows in the Console and in the shader's Inspector.

## Using a shader on a material

Select a material, click the arrow on its **Shader** field and pick the shader, or drag the shader from the Project panel onto the field. A section named after the shader appears under the material's own settings, with a field for each property. Pick **Standard** to go back.

The surface starts as the material's own settings say, so **Colour**, the maps and the rest still count: a shader can tint the material's colour, as above, or replace it.

## Properties

Each field of the shader's struct marked `[Property]` is a setting materials give it. The value after `=` is its default, used by every material that leaves it alone.

| Property | In the Inspector |
| --- | --- |
| `float` | A number |
| `float2`, `float3`, `float4` | Two, three or four numbers |
| `[Color] float3` | A colour with a swatch, picked as the colour picker shows it |
| `[Color] float4` | A colour and its alpha |
| `bool` | A tick box |
| `Picture` | A picture field. Read it with `.sample(uv)`, which gives white for none |

`[Range(low, high)]` keeps a number between the two, and `[Tooltip("...")]` says what a property does when the pointer rests on its field. A shader has room for 16 properties.

## The shader's functions

A shader has any of these three functions, each taking its struct, with the material's values in it, first.

### The surface function

`surface(shader, input, inout surface)` sets what the surface is, pixel by pixel. `input` says where the pixel is:

| `SurfaceInput` | |
| --- | --- |
| `uv` | The texture coordinate |
| `position` | Its place in the world, in metres |
| `normal` | The way the surface faces, in the world |
| `view` | The way to the camera |
| `objectPosition` | Where the Obj is, so each Obj can differ |
| `time` | Seconds since the game started |
| `glass` | Whether the material is **Glass** |
| `thickness` | For **Glass**, how far the view goes through it to what lies behind, in metres: shallow water at the shore, deep water further out. 1000 for anything else |
| `levelPosition` | Its place measured from the level's middle, repeating every 4096 m. Read patterns from it, rather than from `position`, and they stay where they are when the [[Floating Origin]] moves the world's middle |

`surface` starts as the material makes it, for you to change:

| `SurfaceOutput` | |
| --- | --- |
| `color` | The surface's colour |
| `alpha` | How much of it there is. Below a model's cut-out threshold, as on leaves, the pixel is not drawn |
| `normal` | The way it faces, for light and reflections |
| `metallic` | 0 for plastic, wood or stone, 1 for bare metal |
| `roughness` | 0 for a mirror, 1 for matt |
| `emission` | Light it gives off |
| `reflectance` | How much it reflects head on, if not metal: 0.04 for most things, 0.02 for water |
| `transmission` | For **Glass**, how much of what lies behind shows through, by colour. `color` is then light the glass scatters itself, as murky water does |
| `data` | Four numbers of your own, for your light function to read |

### The vertex function

`vertex(shader, input)` moves each vertex, for waving flags, bobbing buoys or swaying grass. It returns how far to move the vertex, in metres, in the world. `VertexInput` has the vertex's `position`, `normal` and `uv`, and the `objectPosition`, `time` and `levelPosition` as above.

```slang
[MaxOffset(0.2)]
struct Bob
{
    [Property] float height = 0.1;
}

float3 vertex(Bob shader, VertexInput input)
{
    return float3(0.0, sin(input.time * 2.0 + input.position.x) * shader.height, 0.0);
}
```

Put `[MaxOffset(metres)]` on the struct to say how far the function moves a vertex at most. The engine skips drawing what is out of view, and needs to know how far outside its shape a moved mesh may reach; without it, a mesh moved far may vanish at the edge of the screen. Shadows move with the vertices.

### The light function

`light(shader, light, surface)` lights the surface your own way, as toon shading does, in place of the Standard shader's lighting. It is called once for the sun and once for each lamp that reaches the pixel, and returns the light that bounces towards the camera. `LightInput` has the `direction` to the light, the light's `color` as it arrives, already shadowed, and the `view` direction to the camera.

```slang
float3 light(Cel shader, LightInput light, SurfaceOutput surface)
{
    float lit = dot(surface.normal, light.direction) > 0.2 ? 1.0 : 0.3;
    return surface.color / PI * light.color * lit;
}
```

Divide the surface's colour by `PI`, as above, for a surface as bright as the Standard shader would draw it.

To hand the light function something the surface function worked out, such as how high on a wave a pixel is, put it in `surface.data`.

Bounced light and reflections still light the surface as the Standard shader does.

## Unlit shaders

Put `[Unlit]` on the struct and the surface is drawn as its `color` plus its `emission`, the same in light and in shadow, as a screen, a glow or a painted sky.

```slang
[Unlit]
struct Screen
{
    [Property, Color] float3 color = float3(1.0, 1.0, 1.0);
    [Property, Range(0, 20)] float brightness = 1.0;
}

void surface(Screen shader, SurfaceInput input, inout SurfaceOutput surface)
{
    surface.color = shader.color * shader.brightness;
    surface.emission = 0.0;
}
```

## In the material file

A material names its shader by the shader's ID, and keeps the values it sets under `[properties]`. A property left at its default is not written, so changing the default in the shader changes every material that never set it.

```toml
color = [1.0, 1.0, 1.0]
shader = "3f9c0b2e8a7d41c6b5e0f1a2d3c4b5a6"

[properties]
tint = [0.2, 0.6, 1.0]
gloss = 0.8
pattern = "5b0e2a7c41d94f3c8a1f0e6b2d9c7a13"
```

Colours are written as the colour picker shows them. Pictures are named by their IDs, as a material's maps are.

## Custom shaders on glass

A material with **Glass** turned on runs its shader too. `input.glass` is true, and `input.thickness` says how much glass, or water, lies between the surface and what is behind it. Set `surface.transmission` to what shows through, and `surface.color` to what the glass scatters itself. An unlit shader's colour adds to what shows through, as a hologram's light does.

## Custom shaders in a build

A build ships every shader in the project, compiled ahead of time, so a game draws with them from its first frame and its players need nothing extra installed. A build compiles shaders again only when one of them, or the engine, has changed.

> [!NOTE]
> A material's shader is compiled into every surface the engine draws, so a project's shaders together cost a little on the GPU, but only the materials using one run its code.
