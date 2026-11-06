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

## Shooting through it

Shots and blasts cut holes in smoke, which fill back in:

- **A shot** cuts a tunnel half a metre wide along its line through every smoke it passes through. Looking down it, you see through the smoke; it fills back in over about a second and a half.
- **A blast** clears a ball round it, all but a trace, which fills back in slowly at first, then faster, over about five seconds.

Holes come from your game's own code, at the moment it fires:

```csharp
// A gun's shot, through any smoke on the way to what it hit.
Smokes.Shoot(muzzle, hit.Point);

// A grenade going off.
Smokes.Blast(Obj.WorldPosition, 4f);
```

`Smokes.SeeThrough(from, to)` says how much of what is at one point shows from another through the smoke between: 1 with none in the way, near 0 hidden. It counts the smoke as thick as it's drawn, as far as it has spread, as it thins away, and with its holes. Use it so enemies can't see the player through smoke but can down a fresh bullet hole:

```csharp
var seen = Smokes.SeeThrough(Obj.WorldPosition, player.WorldPosition) > 0.3f;
```

## What's coming

The smoke drifting and pushing away from explosions, and smoothing its look over frames.
