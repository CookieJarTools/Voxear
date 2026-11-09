---
order: 1
---

An **Ocean** fills the level with sea at its Obj's height, out to the horizon. Add one to any Obj with **Add Component ▸ Water ▸ Ocean**, and move the Obj up or down to raise or lower the sea. Its wind blows along the Obj's blue arrow (+Z), so turn the Obj to turn the waves. A level has one sea: the first Ocean is used.

## Weather

**Weather** picks the sea's mood:

| Weather | The sea |
| --- | --- |
| Calm | A gentle swell under a light breeze, waves about 0.8 m high, hardly any foam |
| Normal | A long swell and a fresh breeze, waves about 2.5 m high, whitecaps here and there |
| Stormy | A big swell under a gale, waves about 6.5 m high, breaking white all over |
| Custom | The winds you set below |

Under **Custom**, the sea is two seas at once, as a real one is: a **Swell** raised by a far-off storm, long and regular, and the choppy sea the local **Wind** raises over it. Each has a wind speed in metres a second and a **Fetch**, how far that wind has blown over open water, in kilometres. A longer fetch makes bigger, longer waves.

## Settings

| Setting | What it does |
| --- | --- |
| Wave scale | How much taller the waves are than the weather makes them: 2 for towering seas. The crests break no more for it |
| Choppiness | How sharp the crests are, from 0 for rounded waves to 1 as real ones |
| Deep | The water's own colour: what it fades into far off under the water, and what deep water shows from above |
| Scatter | The colour sunlight takes through the thin tops of the waves, glowing turquoise towards the sun |
| Foam | How much foam breaking crests make |
| Spray | How much spray flies off breaking crests and the surf, 0 for none |
| Clarity | What a metre of water lets through, by colour: what lies under the water shows through it, bluer and darker the deeper it is |
| Murk | How far you see into the water: 1 as Clarity says, more for murkier water |

## Wind

The sea's wind blows along the Ocean's Obj's blue arrow, as fast as its weather says. Set **Wind Source** to **Wind Zone** for the level's [[Wind|Wind Zone]] to blow it instead: along the Wind Zone's arrow, a strength of 1 a fresh breeze and 3 a gale, so one wind sways the trees and raises the waves. The long swell, which comes from storms far off, keeps to the weather.

When the wind changes, turning, rising or dropping, the waves take **Transition** seconds to follow, the same waves growing, calming and turning rather than jumping to a new sea. Floating Objs ride the same sea as it changes, alike on every machine.

## Quality

The sea's costlier looks can each be turned off, on the Ocean in the Inspector or from code with `Water.Features`, for slower machines or a simpler style. All are on to start with.

| Setting | What it does |
| --- | --- |
| Shores | Waves calm in shallow water, the sea stops at the land, and surf breaks along the shore |
| Foam Trails | Foam stays where crests broke and where surf ran, drifting and ageing into lace |
| 3D Foam | Foam near the camera stands up off the water, its bubbles shading each other |
| Light Bending | What lies under the water shows shifted by the waves, as light bends going into it |
| Light Shafts | Shafts of sunlight under the water, swaying with the waves |
| Wet Lens | Drops on the camera's lens after it comes up out of the water |
| Fine Ripples | The finest waves, from a few centimetres to a metre long |

```csharp
// On a low quality setting: no foam standing up off the water, and no light shafts under it.
Water.Features &= ~(WaterFeatures.Foam3D | WaterFeatures.LightShafts);
```

## How the sea moves

The waves are hundreds of thousands of real ocean waves of every length, from a centimetre to a kilometre, added up on the graphics card each frame by a fast Fourier transform, in six sizes laid over each other so no pattern repeats. The surface round the camera is a fine grid near it that grows coarser with distance, so the waves are sharp at your feet and the sea still reaches the horizon.

Where a crest folds over, it breaks into foam. The foam stays where the crest broke while the wave runs on, streaking down its back, then thins into lace and fades over several seconds. Only the big breakers leave trails; small waves' white is gone as they pass.

Over the ground under the sea, the waves feel the bottom and calm, the longest first, gradually over a few metres where the ground drops away. Near the shore they break as surf once the water is not much deeper than they are tall, and the surf runs up the beach and drains back. Round rocks the water surges white with each wave.

The sea only counts ground up to 4 m above its surface. Anything higher, such as a roof, an arch or a bridge, doesn't stop it, so the sea runs on under a roof to the floor beneath. Walls stop waves as they do in the real world. Over flat shallows such as a flooded floor, the waves have already broken before reaching it, so the water lies nearly still, even with a storm outside.

The water mirrors what's on screen around it, such as pillars, walls and boats. Where the reflection would come from outside the picture, it shows the sky, or the light the GI finds that way.

The waves are worked out the same on every machine, and the game can ask how high the sea is anywhere (see [[Floating Objects]]), so a boat rides the waves you see, alike for every player.

## Under the sea

When the camera goes under the surface, everything fades with distance into the water's own colour, as the water takes the light a colour at a time and its specks scatter it. The water is lit from above: brighter near the surface and looking up, darker looking down and deeper down, and glowing towards the sun, as water scatters light mostly onwards. Clearer water shows further; murkier water glows more and shows less. Sunlight comes down through the waves gathered into caustics, a web of bright lines on everything below that sways as the waves pass: sharp a metre or two down, softer deeper, fading with depth. In the water itself, the light the longer waves gather streams down in shafts from the sun, broken where something stands in its way. Looking up, you see the sky through a bright circle overhead, Snell's window: what stands over the water shows in it too, squeezed towards its edge, and the sun glares through it, spread by the waves. Past its edge the surface turns to a mirror of the water below and what lies in it. Where the camera is half in, a waterline runs across the view.

## Lamps and water

A lamp over the water glints on its ripples in a streak, as the sun does, and lights the water's own colour round it. A lamp under the water glows in the water round it, seen from above or below, and lights what's near it. Its light fades with distance through the water, red first, so a light a few metres down turns everything it lights blue-green.

## Water shader or Ocean?

The built-in **Water** shader (see [[Custom Shaders]]) draws water on a mesh you make, for a fountain, a puddle or a stylised sea of your own shape. An Ocean is the whole sea: endless, its waves real ones that the game can ask about, with foam, Pools, Dry Areas and floating Objs. Use an Ocean for a sea, and a Pool for a lake or a pond.
