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
| Colour | The light's colour |
| Intensity | How bright it is |
| Range | How far the light reaches, in metres, from the nearest point of its shape. It fades out smoothly by then |
| Inner angle, Outer angle | A spot light's cone, in degrees: full brightness inside the inner angle, fading to nothing at the outer |
| Width, Height | A rectangle's size, in metres |
| Radius | A disc's or a tube's radius, in metres |
| Length | A tube's length, end to end, in metres |

An area light's intensity means the same as a point light's: from far away, straight ahead, it gives as much light as a point light of that intensity. Making the shape bigger spreads the same light over more of it, so the light softens rather than brightens.

Intensity is a plain number rather than a real-world unit. New lights start at 3 for a directional light and 10 for the others, and a sun between 2 and 4 suits a daytime scene. Hello's sun is 3.2 and its lamps 6.

When a light is selected, the Scene view shows its reach: the cone of a spot light, the range of a point light, or a directional light's direction. An area light shows its shape, which way it faces, and its range.

## The sun

The first directional light in the level is its **sun**. It is the only light that casts shadows, and the sky draws the sun's disc in its direction and colour. A level only uses one: any other directional lights are left out.

Shadows reach 80 metres from the camera, and are sharpest close to it.

## Using many lights

Point and spot lights are cheap, so a level can have hundreds. Area lights cost a little more each. Each part of the screen is lit by up to 128 lights at once. Only the sun casts shadows yet.

> [!NOTE]
> Light does not bounce from one surface to the next yet. A room lit through a window is lit only where the light falls, plus the soft light of the sky (see [[The Sky]]). Bounced light comes in a later version.
