---
order: 4
---

**Craters** are what shells and grenades dig into the ground. They show on any surface whose material takes craters, wherever it faces up, and they last until the level ends.

## Setting it up

1. Make a **Crater Field** with the Hierarchy's **+ ▸ Crater Field**, and place it in the middle of the ground that gets blasted. Its **Size** is the square, in metres, it covers round itself; craters outside it don't show.
2. Turn on **Takes Craters** in the ground's material. The ground is then diced on the graphics card, as [[Displacement]] is, so craters show even on a flat ground with no height map.

**Resolution** is how finely the field keeps its craters: 1024 over 128 metres keeps them to about 12 centimetres. Higher is sharper and uses more memory.

## Blasting a crater

Craters come from your game's own code, at the moment something goes off:

```csharp
// A shell landing: a crater, and a hole blown through any smoke.
Craters.Blast(hit.Point, 2.5f, 0.8f);
Smokes.Blast(hit.Point, 4f);
```

`Craters.Blast(centre, radius, depth)` digs a bowl `radius` metres across its middle and `depth` metres deep, at most 3, with a lip of earth thrown up round it. Craters that land on each other dig deeper together. It gives back false when the level has no Crater Field.

`Craters.Clear()` fills every crater back in, for a new round.

A field keeps 4096 craters. Past that, each new one fills the oldest back in.

## Things to know

- Colliders don't follow craters, so players walk over the ground as it was. Keep craters shallow where that would show.
- Craters are made while the game runs. They aren't saved with the level.
