---
order: 12
---

Navigation is how characters and creatures find their own way about a level: round walls, up steps, over fords, and round each other.

1. [[Nav Agents]]: send an Obj somewhere and it walks there, going round everything in its way
2. [[Finding Ways from Scripts]]: whether a place can be reached, how far it is on foot, somewhere to wander to

In a [[Block Worlds|Block World]] the places to walk are worked out from the blocks themselves, chunk by chunk, as the world streams in and as blocks are placed and dug. There is nothing to bake. [[Block Mobs]] walk on the same places.

Every part of it is under **Add Component ▸ Navigation**.

## Not done yet

- Levels made of models rather than blocks. Their navigation will be baked in the editor and saved beside the level.
- Jumping gaps, climbing ladders and vines, swimming across deep water and opening doors.
- Ground that costs more or less to cross, such as roads a villager keeps to.
- Seeing the navigation in the Scene view, and Project Settings for it.
