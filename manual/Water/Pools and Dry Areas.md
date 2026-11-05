---
order: 2
---

## Pools

A **Pool** fills a box with water at its Obj's height: a lake, a pond, a flooded room or a castle moat. Add one with **Add Component ▸ Water ▸ Pool**. Its surface is at the Obj's position, and the water reaches **Depth** metres down from it, over a footprint **Size** metres across and along. Turn the Obj to turn the Pool.

A Pool can be anywhere, above or below the sea:

- **Lower than the sea**, a pond in a pit by the shore, the sea stays out of it.
- **Raised above everything**, a block of water standing in the air, its sides and bottom show as walls of water you can see into.
- **Anywhere**, it ripples with the sea's small waves. **Waves** adds the sea's long swell as well, from 0 for none to 1 for as much as the sea has, for a lagoon that rises and falls with the tide.

| Setting | What it does |
| --- | --- |
| Size | Across and along, in metres |
| Depth | How deep the water is, in metres |
| Liquid | Water, or [[Lava]] |
| Waves | How much of the sea's long swell it takes |
| Deep, Scatter, Clarity, Murk | Its water's colours, as [[The Ocean]] describes them |

## Dry Areas

A **Dry Area** keeps water out of its shape: the inside of a boat's hull, a cave under the sea, a diving bell, or a road across a parted sea. Add one with **Add Component ▸ Water ▸ Dry Area** and pick its **Shape**: a box, a ball or a capsule.

The water is cut away pixel by pixel, so a Dry Area on a moving boat keeps the hull dry exactly where it is, with no lag. Where a Dry Area opens into the sea, as a hole in it, its sides show as walls of water, the waves running along their tops.

Inside a Dry Area, the camera is not under water, and scripts asking about the water there find none.

To keep the sea out of a hull, add a Dry Area to the boat the size of its inside. A [[Floating Objects|floating]] boat still floats, as Buoyancy does not ask Dry Areas.
