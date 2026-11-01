---
order: 1
---

# Lights

Add a light with the Hierarchy's **+** button. There are three kinds:

| Light | Shines | Like |
| --- | --- | --- |
| Directional Light | In one direction, everywhere at once | The sun |
| Point Light | Out in every direction from one point | A light bulb |
| Spot Light | In a cone from one point | A torch |

A directional light and a spot light shine the way their Obj faces. Turn the Obj to aim them: while one is selected, the Scene view shows which way it points. Where a directional light stands makes no difference.

## Its settings

Select the light, and the Inspector shows:

| Setting | Does |
| --- | --- |
| Type | Directional, Point or Spot. You can change it at any time |
| Colour | The light's colour |
| Intensity | How bright it is |
| Range | How far a point or spot light reaches, in metres. It fades out smoothly by then |
| Inner angle, Outer angle | A spot light's cone, in degrees: full brightness inside the inner angle, fading to nothing at the outer |

Intensity is a plain number rather than a real-world unit. New lights start at 3 for a directional light and 10 for the others, and a sun between 2 and 4 suits a daytime scene. Hello's sun is 3.2 and its lamps 6.

When a light is selected, the Scene view shows its reach: the cone of a spot light, the range of a point light, or a directional light's direction.

## The sun

The first directional light in the level is its **sun**. It is the only light that casts shadows, and the sky draws the sun's disc in its direction and colour. A level only uses one: any other directional lights are left out.

Shadows reach 80 metres from the camera, and are sharpest close to it.

## Many lights

Point and spot lights are cheap, so a level can have hundreds. Each part of the screen is lit by up to 128 of them at once. Point and spot lights do not cast shadows yet.

> [!NOTE]
> Light does not bounce from one surface to the next yet. A room lit through a window is lit only where the light falls, plus the soft light of the sky (see [[The Sky]]). Bounced light comes in a later version.
