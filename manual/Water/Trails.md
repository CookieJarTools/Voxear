---
order: 6
---

## Trails on the water

Whatever moves on water leaves a trail behind it. A swimmer leaves ripples and a short wake of foam, and rings spread from it as it treads water. A boat, a raft or a crate pushed across leaves a V of waves that spreads out and fades, with white water down its middle. Rings spread from where something drops into the water or climbs out of it. Trails lie on every water there is: the [[The Ocean|sea]], [[Pools and Dry Areas|Pools]], [[Streams]] and the water of [[Block Worlds|worlds of blocks]], and down a river they drift with the current.

Nothing needs setting up:

- Every Obj with a **Character Controller** stirs the water it stands, wades or swims in, as wide as the character.
- Every Obj with a **Collider** and a **Rigidbody** stirs it where it cuts the surface, as wide as its Collider there. A long one, such as a boat's hull, stirs it along its length.

The faster something moves across the water, the narrower its V and the more foam it throws up. Something carried along by a river at the river's own speed leaves no wake; only what moves against the water stirs it.

## Water Wake

A **Water Wake** tunes the trail its Obj leaves. Add one with **Add Component ▸ Water ▸ Water Wake**.

| Setting | What it does |
| --- | --- |
| Size | How wide the wake is where the Obj cuts the water, in metres. 0 goes by its body: its Character Controller or Collider, or half a metre with neither |
| Strength | How hard it stirs the water: 1 as its body would, 2 for waves twice as high, 0 for none |
| Foam | How much foam it leaves: 1 as its body would, 0 for clear water behind it |

Switch the Water Wake off with its tick in the Inspector and the Obj leaves no trail at all, even one its body would. An Obj with a Water Wake and no body, such as a ship's mesh moved by a script, stirs the water where it crosses it, as wide as Size says.

A script reaches it through `Obj.WaterWake`, and gives an Obj one with `Obj.AddWaterWake()`:

```csharp
// A speedboat throws up more white water the faster it goes, and a ghost ship leaves none.
var wake = Obj.AddWaterWake();
wake.Foam = Mathf.Clamp(speed / 8f, 0.5f, 3f);
ghostShip.WaterWake.Enabled = false;
```

| Member | What it is |
| --- | --- |
| `Size` | The wake's width in metres; 0 for as wide as its body |
| `Strength` | How hard it stirs the water |
| `Foam` | How much foam it leaves |
| `Enabled` | Whether it stirs the water at all |
| `Remove()` | Takes it off; the Obj then stirs the water as its body does |

## How trails are worked out

Trails are worked out on the GPU over a square of water round the camera, and only drawn: nothing floats on them, and they are not the same on every machine. Set them in **Project Settings ▸ Graphics ▸ Water Trails**:

| Setting | What it does |
| --- | --- |
| Quality | Off, Low, Medium or High: 256, 512 or 1024 texels a side of the square. Higher draws finer ripples, for more GPU time and memory. Off leaves no trails |
| Area | How far across the square round the camera is, in metres. Larger keeps wakes further off, with coarser ripples at the same Quality |
| Look | Match the Water, Smooth or Blocky. Blocky stands the water in small squares a quarter of a block across, each flat, stepping up and down with the ripples, its foam laid square by square. Match the Water is blocky on worlds of blocks and smooth on the sea, Pools and Streams |

On a 1920×1080 frame, Medium costs about 0.05 ms of GPU time each frame there is something to stir, and nothing once the water has been still for twenty seconds.

From the command line, `vox editor call project.set key=water_trails.quality value=High` changes it in a running editor, as Project Settings does, and the editor's tools for AI reach the same settings and every Water Wake's.
