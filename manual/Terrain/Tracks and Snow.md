---
order: 11
---

Feet, wheels and sled runners leave **tracks** in ground that gives: footprints across fresh snow, ruts behind a cart in mud. A track stays behind whatever made it, and where it is pressed deep the ground under the snow shows through. Snow can fill its tracks in again as it falls.

## Making tracks

Add a **Track Maker** to whatever should leave tracks (Add Component, then Terrain), and put its Obj where the thing meets the ground: an empty Obj at the sole of each foot, or under a wheel's hub where the tyre touches. It presses into the ground while it touches it, and leaves nothing while it is in the air, so a walking character's feet leave footprints one by one.

| Setting | What it does |
| --- | --- |
| Width | How wide its track is, in metres: about 0.15 for a boot, 0.3 for a cart's wheel |
| Depth | How deep it presses into ground that gives all the way, in metres. Less into firmer ground |

## Ground that gives

Tracks press only into surfaces that take them. Each of the terrain's surfaces has two settings for that, beside its others (see [[Surfaces]]):

| Setting | What it does |
| --- | --- |
| Tracks | How far Track Makers press into it, as a share of their depth: 1 for fresh snow, about a half for mud, 0 for rock and road |
| Fills in | How many seconds its tracks take to fill in again, as falling snow fills them. 0 keeps them |

Where tracks press deep into a surface that gives, the terrain's first surface, the ground under the rest, shows through at the bottom: mud at the bottom of footprints in thin snow. For snow too deep for tracks to reach the ground, make the snow the terrain's first surface, so there is nothing under it to show.

Pressed ground is packed tighter, so it is drawn a little darker and smoother than the ground round it.

## Trodden grass

[[Grass]] lies flat wherever a Track Maker has trodden, on any surface, even rock and road. It stands up again over its kind's **Springs back** time: a few seconds for stiff grass, a minute or more for soft grass, 0 for grass that stays flat.

## How tracks are drawn

Tracks are only drawn. Nothing collides with them, so a character walks on the ground as it was, and they are not the same on every machine: each player's game makes its own from what it sees. For ground changed for good, the same for every player, change the terrain itself (see [[Changing Terrain from Scripts]]).

Each view keeps its tracks in a map of the ground round its camera, 64 m across, a texel every 6 cm. The ground's pixels look into it to draw the floor of each track where the eye meets it, with its sides lit by their slope, and each blade of grass looks into it to know whether to lie down. Tracks are kept only while they are on the map: the camera moving more than 32 m from a track forgets it, and the grass there stands up at once.

At most 64 Track Makers press into the ground at once, those nearest the camera.
