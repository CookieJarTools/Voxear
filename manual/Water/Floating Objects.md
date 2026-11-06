---
order: 5
---

## Buoyancy

A **Buoyancy** floats its Obj on the sea, on Pools and on Streams: a crate bobs in the waves, a boat rocks and rights itself, and a log is carried down a river. Add one with **Add Component ▸ Water ▸ Buoyancy**. It needs a [[Colliders and Rigidbodies|Collider and a Rigidbody]], and adds them if the Obj has none.

The water pushes up on nine upright columns across the Obj's Collider, three by three, each as tall as the Collider and pushed as much as it is under the water there. A wave under one end lifts that end, so the Obj rocks with the sea, and when it tilts, the side lower in the water is pushed harder, so it rights itself. The water also slows it against the way the water itself moves, so it rides the waves rather than lagging under them, and a Stream carries it along.

| Setting | What it does |
| --- | --- |
| Submerged | How much of it sits under still water, from 0 to 1: about 0.5 for a boat, more for a waterlogged log. Over 1, it sinks. Its mass does not change this |
| Drag | How quickly the water slows it, and so how soon it stops bobbing and how fast a Stream carries it: 1 stops it bobbing with no overshoot, less lets it bob for longer |
| Angular drag | The same for turning: how soon it stops rocking |

Make the Collider the shape of what floats: a box round a boat's hull, not round its mast. Buoyancy does not ask [[Pools and Dry Areas|Dry Areas]], so a boat with a Dry Area keeping water out of its hull still floats.

Floating Objs feel the sea floor through Colliders: near a shore, the waves calm where the water is shallow, so give the ground Colliders, or a floating Obj rides the open sea's waves over water drawn calm.

A script reaches it through `Obj.Buoyancy`, and gives an Obj one with `Obj.AddBuoyancy()`:

```csharp
// A barrel that fills with water and sinks.
protected override void FixedUpdate()
{
    var buoyancy = Obj.Buoyancy;
    buoyancy.Submerged += 0.02f * Time.FixedDelta;
}
```

## Asking about the water from scripts

The `Water` class asks about the water anywhere in the level: the sea's waves exactly as they are drawn, worked out the same on every machine, and Pools and Streams:

```csharp
protected override void Update()
{
    // A buoy riding the waves.
    var at = Transform.Position;
    Transform.Position = new Vector3(at.X, Water.HeightAt(at), at.Z);
}
```

| Member | What it gives |
| --- | --- |
| `Water.HeightAt(position)` | How high the water's surface is over a point. Negative infinity where there is no water |
| `Water.NormalAt(position)` | The way the surface faces there |
| `Water.FlowAt(position)` | How fast the water flows there, in metres a second: a Stream's current, or on the sea the water carried round by its waves |
| `Water.IsUnderwater(position)` | Whether a point is under the surface |
| `Water.TrySample(position, out WaterPoint water)` | All three at once, and whether there is water there |

Inside a Dry Area there is no water. Over a Pool, its surface counts, not the sea's.
