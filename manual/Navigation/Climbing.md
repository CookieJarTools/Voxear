---
order: 8
---

Nav Agents climb vines, ropes and chains in a [[Block Worlds|Block World]] by themselves: up a cliff a vine hangs down, down a rope into a pit, and over the ledge at the top. Nothing needs a [[Nav Links|Nav Link]].

## Making a ladder

Any block can be climbed as a ladder:

1. Make a block, as a few boxes of a ladder's rungs, or any shape.
2. Turn on **Climbable** under its other settings in the Inspector.

Stand the blocks one over another against a wall, and agents climb them. A climbed block is no ground to navigation, so nothing stands on its top or walks into it; bodies still meet its boxes.

## How agents climb

A way up a vine is a climb from its foot to its top, all of it at once, and a step off over the ledge, each crossed as a Nav Link of the kind Climb is: slowly, along its line, whatever moves the agent. A vine side by side with another is not crossed sideways in the air.

Each block climbed costs twice what a block walked does, so a way round of no more than twice as far is taken first.

Turn **Climbs** off on a Nav Agent for one that does not climb, as a horse. [[Block Mobs]] walking as agents of their own do not climb.

Turn on **Gizmos ▸ Navigation** to see what is climbed: a light green square at each hold.
