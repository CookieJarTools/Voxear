---
order: 2
---

## Pools

A **Pool** fills a box with water at its Obj's height: a lake, a pond, a flooded room or a castle moat. Add one with **Add Component ▸ Water ▸ Pool**. Its surface is at the Obj's position, and the water reaches **Depth** metres down from it, over a footprint **Size** metres across and along. Turn the Obj to turn the Pool.

A Pool can be anywhere, above or below the sea:

- **Lower than the sea**, a pond in a pit by the shore, the sea stays out of it.
- **Raised above everything**, a block of water standing in the air, its sides and bottom show as walls of water you can see into.
- **Anywhere**, it ripples with the sea's small waves. **Waves** adds the sea's long swell as well, from 0 for none to 1 for as much as the sea has, for a lagoon that rises and falls with the tide.

A large Pool is drawn as finely near the camera as the sea is, so a lake's ripples are as sharp at your feet as a pond's.

| Setting | What it does |
| --- | --- |
| Size | Across and along, in metres |
| Depth | How deep the water is, in metres |
| Liquid | Water, or [[Lava]] |
| Waves | How much of the sea's long swell it takes |
| Deep, Scatter, Clarity, Murk | Its water's colours, as [[The Ocean]] describes them. A new Pool starts as a lake's green |

## Dry Areas

A **Dry Area** keeps water out of its shape: the inside of a boat's hull, a cave under the sea, a diving bell, or a road across a parted sea. Add one with **Add Component ▸ Water ▸ Dry Area** and pick its **Shape**: a box, a ball, a capsule, or a mesh.

The water is cut away pixel by pixel, so a Dry Area on a moving boat keeps the hull dry exactly where it is, with no lag. Where a Dry Area opens into the sea, as a hole in it, its sides show as walls of water, the waves running along their tops.

Inside a Dry Area, the camera is not under water, and scripts asking about the water there find none.

To keep the sea out of a hull, add a Dry Area to the boat the size of its inside. A [[Floating Objects|floating]] boat still floats on the sea round it, as Buoyancy does not ask the Dry Areas on its own Obj or under it. Anything else floating inside a Dry Area, such as cargo in the hold, floats on its Bilge, or sits on the floor with none.

### Mesh

A Dry Area with the **Mesh** shape keeps water out of the inside of a closed mesh, placed, turned and scaled with its Obj, so it fits a curved hull exactly. Its **Mesh** is the Obj's own Mesh Renderer's to start with; pick another for a simple closed shape of the hull's inside, up to its deck, which is not drawn. An open hull's own mesh has no inside to keep dry, so give it one of its own.

The mesh's walls do not show as water, as a box's do: the hull is what shows there. Scripts asking about water inside it go by the box round the mesh.

### Bilge

Set a Dry Area's **Bilge** to the depth of water standing in it, in metres from its bottom: water shipped over the side of a boat, or a flooded cellar. It stays level as the hull rocks, as high over the Dry Area's middle as when upright, and fills only the Dry Area's inside. It has no walls of its own: the hull holds it. Scripts asking about the water in a Dry Area find its bilge, the camera goes under it, and cargo in the hold floats on it. 0 leaves it dry, and so does switching off **Bilge on**, which keeps the depth for when it is switched on again.

The bilge is worked out again every frame from these two settings, with nothing made again when they change, so a script can raise it a little each frame as a hold floods. However shallow it is, it lies clear on the floor: its ripples calm as the water thins, and it foams only against what stands or floats in it.

| Setting | What it does |
| --- | --- |
| Shape | Box, Sphere, Capsule or Mesh |
| Size, Width, Height | How big the shape is, in metres, as the shape has it |
| Mesh | For the Mesh shape: the closed mesh it keeps water out of; empty for the Obj's Mesh Renderer's |
| Bilge | How deep the water standing in it is, in metres from its bottom |
| Bilge on | Whether that water is there at all |

## Dry Areas in scripts

A script reaches a Dry Area through `Obj.DryArea`, which is null when the Obj has none, and gives an Obj one with `Obj.AddDryArea()`. Set its bilge as a depth in metres with `Bilge`, or as a share of how tall its inside stands with `BilgeFill`: the water drawn in the hold, what `Water` finds there, and what floats in it follow on the same frame.

```csharp
// A hold that floods through a hole in the hull, a little more each second, until it is pumped dry.
public sealed class Leak : Behaviour
{
    public float PerSecond = 0.02f;

    protected override void Update()
    {
        if (Obj.DryArea is { } hold)
        {
            hold.BilgeFill = MathF.Min(hold.BilgeFill + PerSecond * Time.Delta, 0.8f);
        }
    }

    public void Pump() => Obj.DryArea?.Bilge = 0f;
}
```

| Member | What it is |
| --- | --- |
| `Shape` | The `DryShape` it keeps water out of: `Box`, `Sphere`, `Capsule` or `Mesh` |
| `Size` | How big its shape is, in metres: a box's three sides, a ball's width in X, a capsule's width in X and height in Y |
| `Bilge` | How deep the water standing in it is, in metres from its bottom; 0 for none |
| `BilgeFill` | The same as a share of `InsideHeight`, from 0 for dry to 1 for full |
| `BilgeEnabled` | Whether the bilge is there; off keeps `Bilge` for when it is on again |
| `InsideHeight` | How tall its inside stands, in metres: the deepest the bilge can be. A Mesh's is 0 until its mesh has loaded |
| `IsValid` | False once the Dry Area has been taken off, or its Obj is gone |
| `Remove()` | Takes the Dry Area off, letting the water back in |

A Mesh Dry Area's mesh is picked in the Inspector; set to `Mesh` from a script, it takes the Obj's Mesh Renderer's.

The Bilge is saved with the scene. A player's [[Saving the Game|save]] keeps the level as it keeps the rest of a script's own state: in a `[Saved]` field of the script that floods the hold, which sets the Bilge from it.

From outside the game, `vox editor call field.set` and `vox mcp` set the same values as the Inspector, while the game plays too: the component `dry_area`, and its fields `shape`, `size`, `mesh`, `bilge` and `bilge_on`.
