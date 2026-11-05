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
| Wave scale | How much taller the waves are than the weather makes them: 2 for Sea of Thieves' towering seas. The crests break no more for it |
| Choppiness | How sharp the crests are, from 0 for rounded waves to 1 as real ones |
| Deep | The colour of the water seen deep down, lit by the sky |
| Scatter | The colour sunlight takes through the thin tops of the waves, glowing turquoise towards the sun |
| Foam | How much foam breaking crests make |
| Clarity | What a metre of water lets through, by colour: what lies under the water shows through it, bluer and darker the deeper it is |
| Murk | How far you see into the water: 1 as Clarity says, more for murkier water |

## How the sea moves

The waves are made the way Sea of Thieves and Atlas make theirs: hundreds of thousands of real ocean waves of every length, from a few centimetres to a kilometre, added up on the graphics card each frame by a fast Fourier transform, in four sizes laid over each other so no pattern repeats. The surface round the camera is a fine grid near it that grows coarser with distance, so the waves are sharp at your feet and the sea still reaches the horizon.

Where a crest folds over, it breaks into foam. The foam stays where the crest broke while the wave runs on, streaking down its back and fading over a couple of seconds, as Sea of Thieves' does. Only the big breakers leave trails; small waves' white is gone as they pass. Where the sea thins over a shore, it laps white too.

The waves are worked out the same on every machine, and the game can ask how high the sea is anywhere (see [[Floating Objects]]), so a boat rides the waves you see, alike for every player.

## Under the sea

When the camera goes under the surface, the world turns blue and dim with distance, as the water takes the light a colour at a time. Sunlight comes down through the waves in a moving web of caustics on everything below, and fades with depth. Looking up, you see the sky through a bright circle overhead, Snell's window, and past its edge the surface turns to a mirror of the water below. Where the camera is half in, a waterline runs across the view.

## Water shader or Ocean?

The built-in **Water** shader (see [[Custom Shaders]]) draws water on a mesh you make, for a fountain, a puddle or a stylised sea of your own shape. An Ocean is the whole sea: endless, its waves real ones that the game can ask about, with foam, Pools, Dry Areas and floating Objs. Use an Ocean for a sea, and a Pool for a lake or a pond.
