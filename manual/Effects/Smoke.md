---
order: 2
---

**Smoke** fills the space round its Obj, as a smoke grenade does: it spreads out from the Obj, pours through doorways and round corners, and stays out of walls. Make one with the Hierarchy's **+ ▸ Smoke**, or add one to any Obj with **Add Component ▸ Effects ▸ Smoke**. It fills straight away in the Scene view too, and fills again whenever you move it or change it.

## How it fills

Smoke fills as much space as a ball of its **Size** would hold, nearest first, so in the open it's a ball and in a room it fills the room. What doesn't fit inside pours out of the doors and windows. Anything with a Collider that stays put stops it, however thin. Objs that physics moves, such as crates, and triggers don't.

**Cell Size** is how fine the smoke's cells are. Finer cells follow walls more closely, but a smoke has 32 cells along each side, so finer cells also reach less far. Half a metre or less suits a grenade.

## How it looks

- **Density**: how thick it is. Higher hides more.
- **Colour**: how much of the light it scatters. Grey for smoke, white for steam, dark for soot.
- **Spread Time**: how many seconds it takes to spread out from its Obj to its full size.
- **Lasts** and **Fade Time**: how long it stays before it thins away, and how long it takes to thin. A Lasts of 0 keeps it for ever.
- **Seed**: two smokes that are alike but for their Seed swirl differently.

The sun lights it through itself, so its far side is darker, and objects in front of it cast shadows into it. It takes the light round it from the sky and bounced light, and it sits in the scene's fog as far off as it is.

## What's coming

Bullets and blasts cutting holes through it that fill back in, asking from scripts whether a line of sight passes through it, and the smoke drifting and pushing away from explosions.
