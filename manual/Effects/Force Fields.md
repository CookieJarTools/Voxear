---
order: 2
---

A **Force Field** moves the particles round its Obj: it pulls them in, drives them away, swirls them or blows them along. Make one with the Hierarchy's **+ ▸ Force Field**, or add one to any Obj with **Add Component ▸ Effects ▸ Force Field**. While it's selected, the Scene view shows how far it reaches.

A Force Field only moves the particles of Particle Systems that take forces. Each system's **Forces** setting says how much: 0, as a new system has it, ignores every field, and 1 takes them in full (see [[Particle Systems]]).

## What a Force Field does

| Kind | What it does to particles in reach |
| --- | --- |
| Attract | Pulls them toward its Obj |
| Repel | Drives them away from its Obj |
| Vortex | Swirls them round its Obj's green arrow (Y) |
| Blow | Blows them along its Obj's blue arrow (Z), as a fan or a draught does |

- **Strength**: how hard it moves a particle at its middle, in metres a second faster each second. It's weaker further out, and nothing at its Radius.
- **Radius**: how far it reaches, in metres.

Turn the Obj to aim a Vortex or a Blow. Move it, and the particles follow: a Vortex on a spinning blade drags the dust round with it. Up to 16 fields and blasts move particles at once.

## Blasts

A blast throws particles out from a point for a moment, as an explosion does. Scripts set one off with `Particles.Push`: where, how wide, and how fast it throws the particles at its middle, in metres a second.

```csharp
// A grenade throws the smoke and dust round it outwards.
Particles.Push(grenade.Position, 6f, 12f);
```

## What passes through

Anything with a **Foliage Interaction**, as a player or a vehicle has to part the grass, also drives particles out of its way: smoke and dust that take forces part round whoever runs through them.

## From scripts

`Obj.ForceField` is the Obj's Force Field, and `Obj.AddForceField()` gives an Obj one. `Kind`, `Strength` and `Radius` can be read and changed.

```csharp
// A whirlwind that picks up the dust round it.
var whirl = Obj.AddForceField();
whirl.Kind = ForceKind.Vortex;
whirl.Strength = 15f;
whirl.Radius = 4f;
```
