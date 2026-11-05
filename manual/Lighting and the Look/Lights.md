---
order: 1
---

Add a light with the Hierarchy's **+** button. There are six kinds:

| Light | How it shines | Like |
| --- | --- | --- |
| Directional Light | In one direction, everywhere at once | The sun |
| Point Light | Out in every direction from one point | A light bulb |
| Spot Light | In a cone from one point | A torch |
| Rectangle Light | From one face of a rectangle | A window, a screen, or a panel in a ceiling |
| Disc Light | From one face of a disc | A round ceiling light |
| Tube Light | Out from a long, thin capsule | A fluorescent tube, or a neon strip |

Directional, spot, rectangle and disc lights shine the way their Obj faces. Turn the Obj to aim them: while one is selected, the Scene view shows which way it points. Where a directional light stands makes no difference. A tube lies along its Obj's red (X) axis.

Rectangle, disc and tube lights are **area lights**. They light from a shape rather than a point, so their highlights on shiny surfaces take that shape, and the light round them falls off softly.

## A light's settings

Select the light, and the Inspector shows:

| Setting | What it does |
| --- | --- |
| Type | Directional, Point, Spot, Rectangle, Disc or Tube. You can change it at any time |
| Appearance | **Colour**, to give the light a colour of its own, or **Filter and Temperature**, to light in the colour of a temperature, which Colour then tints |
| Colour | The light's colour, or with a temperature, a filter over it: white leaves the temperature's colour as it is |
| Temperature | With Filter and Temperature, the light's colour temperature in kelvin, as below |
| Intensity | How bright it is |
| Range | How far the light reaches, in metres, from the nearest point of its shape. It fades out smoothly by then |
| Shadows | Whether the light casts shadows, for any light but a directional one. On by default: see below |
| Inner angle, Outer angle | A spot light's cone, in degrees: full brightness inside the inner angle, fading to nothing at the outer |
| Width, Height | A rectangle's size, in metres |
| Radius | A disc's or a tube's radius, in metres |
| Length | A tube's length, end to end, in metres |
| Cookie | A picture the light shines through, for a spot or directional light |
| Cookie size | How many metres a directional light's cookie covers before it repeats |
| IES profile | How bright a real light fitting is in each direction, for a point or spot light |

## Colour temperature

Real lights are coloured by how hot they burn: a candle flame is orange, a household bulb warm white, the noon sun white and the light from a clear sky blue. Give a light a **temperature** instead of picking its colour, and it lights in that colour. The temperature changes only the colour, never how bright the light is.

| Temperature | Looks like |
| --- | --- |
| 1900 K | A candle, or firelight |
| 2700 K | A household bulb |
| 3200 K | A halogen lamp, or the sun an hour before sunset |
| 4000 K | A cool white tube |
| 5500 K | The midday sun |
| 6500 K | Daylight under a light overcast |
| 10000 K | The blue light of a clear sky, or shade on a sunny day |

In a scene file:

```toml
[obj.light]
type = "point"
temperature = 1900
```

With a temperature, `color` filters it, as a gel over a lamp does: `color = [1.0, 0.8, 0.8]` with 6500 K gives a pinkish daylight.

An area light's intensity means the same as a point light's: from far away, straight ahead, it gives as much light as a point light of that intensity. Making the shape bigger spreads the same light over more of it, so the light softens rather than brightens.

Intensity is a plain number rather than a real-world unit. New lights start at 3 for a directional light and 10 for the others, and a sun between 2 and 4 suits a daytime scene. Hello's sun is 3.2 and its lamps 6.

When a light is selected, the Scene view shows its reach: the cone of a spot light, the range of a point light, or a directional light's direction. An area light shows its shape, which way it faces, and its range.

## Cookies

A cookie is a picture a light shines through, as light falls through a window frame or the leaves of a tree. Black in the picture blocks the light, white lets it through, and colours tint it.

- On a **spot light**, the picture is stretched over the light's cone, so the whole picture shows where the light lands.
- On a **directional light**, the picture is laid over the world from above and repeats every **Cookie size** metres. Use a soft, dappled picture for sunlight through a forest's canopy.

Pick any picture in your project as a cookie. Pictures with soft edges look best, since the light shows every hard edge.

## IES profiles

Real light fittings are not equally bright in every direction: a downlight throws most of its light down, and a wall washer lights the wall beside it. Lamp makers publish how bright each fitting is in each direction as an **IES file** (`.ies`). Put one in your project, then pick it as a point or spot light's **IES profile**, and the light shines as the real fitting does.

The profile's straight-ahead direction is the way the light faces, along its Obj's blue (Z) axis. The light's intensity still sets how bright it is overall: the profile only shapes it.

## Shadows from lamps

Lights cast shadows unless you turn their **Shadows** off: what stands in a light's way casts a shadow, so a candle behind a pillar lights the floor round the pillar but not behind it. With Shadows off, a light shines through walls, curtains and everything else, which costs nothing and often looks fine for small, soft lights.

Each light that casts shadows draws what is round it again, so they cost more than the light itself:

- A **spot light** draws its shadow once, along its cone.
- Any other light draws it six times, once for each side of a cube round it.

In each frame, the eight lights nearest the camera that ask for shadows cast them, as long as their views fit the shadow atlas (48 views, so up to eight spot lights, or eight point lights). Any more light without a shadow until the camera comes nearer.

Nothing within 10 centimetres of a light casts its shadow, or just past its own shape for an area or tube light, so a flame or bulb drawn round a light does not shut its light in. Glass casts no dark shadow, but colours the light through it, as the sun's. An opaque shade does, as a real one would: the light comes out of its open end.

```toml
[obj.light]
type = "point"
temperature = 1900
shadows = false
```

## The sun

The first directional light in the level is its **sun**. It always casts shadows, and the sky draws the sun's disc in its direction and colour. A level only uses one: any other directional lights are left out.

At night, set its **Sky body** to Moon: the sky draws the moon instead, and darkens so the stars show. See [[The Sky]].

Shadows reach 80 metres from the camera, unless the scene's lighting says otherwise.

### Shadow distance

The sun's shadows are drawn into four shadow maps, each covering a stretch of the view from near the camera outwards. Two settings on the Lighting page's Sun tab, or in the lighting file's `[sun]`, decide how:

| Setting | What it does |
| --- | --- |
| Shadow distance | How far from the camera the sun casts shadows, in metres, 80 by default. The same four maps cover a shorter distance more finely, so a close, indoor scene can bring it in to 30 for sharper shadows; a wide landscape can push it out to 200 or more, each shadow a little softer |
| Cascade split | How the four maps share that distance, from 0 to 1, 0.85 by default. Near 1, the nearest map is small and very sharp and the farthest wide; near 0, they share it evenly, so shadows far off are sharper and those by the camera softer |

```toml
[sun]
shadow_distance = 40.0
shadow_split = 0.7
```

### Soft shadows

A real sun is a disc, not a point, so shadows are sharp where things touch the ground and soften the farther they fall from what casts them. The sun's **Shadow softness** sets how wide the sun looks, in degrees:

- **0.5**, the default, is about the real sun.
- Higher, up to a few degrees, gives the softer shadows of a hazy day.
- **0** gives hard edges everywhere, and costs least.

### Shadow tint

In real light, shade is whatever colour the sky and the bounced light make it. Art often paints it a colour of its own instead: warm brown under trees, purple at dusk. The sun's **Shadow tint** adds that colour where the sun does not reach, in its shadows and on the sides of things turned away from it. Black, the default, adds nothing.

Pick a dark, gentle colour: the tint is lit by the sun's colour and strength, so a bright one lights shade almost as much as sunlight. Some of it also shows on lit ground where the sun falls at a slant, as the light turns away.

```toml
[obj.light]
type = "directional"
shadow_tint = [0.42, 0.30, 0.38]
```

### Contact shadows

Where small things meet, such as a cup on a table or the feet of a chair, the shadow map is too coarse to show the shadow. **Contact shadows** fill these in, by looking a short way towards the sun from each pixel. They are on by default, and set in the scene file's post-processing:

```toml
[post.contact_shadows]
enabled = true
length = 0.3      # how far each pixel looks towards the sun, in metres
thickness = 0.15  # how thick things count as, in metres
```

Contact shadows work beyond the 80 metres shadows reach, so distant things still sit on the ground. They cost about a quarter of a millisecond at 1440p.

## Using many lights

Point and spot lights are cheap, so a level can have hundreds. Area lights cost a little more each. Each part of the screen is lit by up to 128 lights at once. Only the sun casts shadows yet.

> [!NOTE]
> Unless you turn on bounced light, light does not bounce from one surface to the next: a room lit through a window is lit only where the light falls, plus the soft light of the sky. See [[Bounced Light]].
