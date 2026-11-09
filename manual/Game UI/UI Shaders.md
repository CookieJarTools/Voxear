---
order: 7
---

A UI element's background can be drawn by a shader instead of a flat colour: a band of light sweeping over a loading bar, stripes moving along a busy one, scan lines on a terminal. Give the element `shader` in its style. The engine has four effects built in, and you can write your own.

Shaders work the same everywhere Voxear UI draws: on the screen, on [[UI Panels in the World]], and in the editor's own windows.

```css
.loading {
    background-color: #2a2b31;
    shader: shimmer;
    --speed: 1.5;
}

.terminal {
    background-color: #0b2a14;
    shader: url("Shaders/Static.shader");
}
```

The shader draws only the background. The element's border, rounded corners and opacity still come from its style, and so does its text.

## Built-in effects

| Effect | What it draws | Properties |
| --- | --- | --- |
| `shimmer` | A band of light sweeping across, for loading and highlights | `--speed` 0.6 (crossings a second), `--width` 0.3 (of the box), `--shine` (its colour) |
| `stripes` | Stripes moving across, for busy bars and warning tape | `--stripe` (their colour), `--spacing` 14 (pixels), `--speed` 24 (pixels a second), `--angle` 45 (degrees) |
| `scanlines` | Dark lines and a faint flicker, for screens and terminals | `--spacing` 3 (pixels), `--strength` 0.25, `--flicker` 0.04 |
| `static` | Grain that changes as it plays | `--amount` 0.12, `--speed` 24 (changes a second) |

Each effect works on the element's own `background-color`, so a hover or a transition that changes the colour still shows through.

A built-in effect costs nothing until an element uses it.

## Shader properties

A shader's properties are the element's custom properties of the same names: a shader's `speed` is the element's `--speed`. A property the element leaves out takes the shader's default. Write them in the same rule as the shader, or anywhere a variable reaches the element, such as `:root`:

```css
.warning-tape {
    background-color: #e8b64c;
    shader: stripes;
    --stripe: #1d1e22;
    --spacing: 24;
    --speed: 0;
}
```

From a script, set them with the rest of the element's style:

```csharp
bar.SetStyle("shader: stripes; --speed: 60");
```

Properties don't animate with `transition` yet. The background and border colours do, and a shader is given both.

## Writing a UI shader

Right-click in the Project panel and choose **New Shader… ▸ UI**. A UI shader is a `.shader` file, written in Slang as a material's custom shader is (see [[Custom Shaders]]), with a `ui` function instead of `surface`:

```slang
struct Glow
{
    [Property, Color] float4 glow = float4(1.0, 1.0, 1.0, 0.4);
    [Property, Range(0, 4)] float speed = 0.5;
}

float4 ui(Glow shader, UiInput input)
{
    float at = frac(input.time * shader.speed) * 1.6 - 0.3;
    float band = saturate(1.0 - abs(input.uv.x - at) / 0.3);
    float4 light = shader.glow * band * band;
    return light + input.background * (1.0 - light.a);
}
```

`ui` returns the background's colour at one pixel, premultiplied by its alpha: multiply a colour's red, green and blue by its alpha before returning it. Colours the shader is given are already that way.

An element names a UI shader by its path, as `shader: url("Shaders/Glow.shader")`, or by its ID.

Save the file and the editor compiles it in the background, as it does a material's shader. Until it's ready, elements using it draw their plain background.

### What a UI shader is given

| `UiInput` | |
| --- | --- |
| `position` | Where the pixel is in the element's box, in pixels from its top left |
| `size` | The box's size, in pixels |
| `uv` | Where the pixel is from 0 to 1 across and down |
| `background` | The element's background colour |
| `border` | The element's border colour |
| `time` | Seconds since the game started |
| `scale` | Screen pixels per UI pixel, for detail a pixel wide on a high-resolution screen |

### UI shader properties

Properties are written as a material shader's are: a number, two to four numbers, a `[Color] float3` or `float4`, or a `bool`. Colours are written in a sheet as anywhere else, as `#ffcc00` or `oklch(…)`, and arrive made linear. A bool is written `--on: true` or `--on: 1`. A UI shader can't have Picture properties.

A shader draws either UI or a surface, so a UI shader has no `surface`, `vertex` or `light` function, and materials can't pick it.

## UI shaders in a build

A build compiles the project's UI shaders ahead of time with its material shaders, so the game draws them from its first frame.
