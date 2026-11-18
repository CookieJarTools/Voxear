---
order: 12
---

Navigation is how characters and creatures find their own way about a level: round walls, up steps, over fords, and round each other.

1. [[Nav Agents]]: send an Obj somewhere and it walks there, going round everything in its way
2. [[Finding Ways from Scripts]]: whether a place can be reached, how far it is on foot, somewhere to wander to
3. [[Areas and Costs]]: roads that are kept to, ground that is kept off, and ground nothing walks on
4. [[Nav Links]]: ladders, gaps, drops and doors that take you elsewhere, joining places walking does not
5. [[Levels of Models]]: baking the navigation of a level built from models and colliders
6. [[Nav Obstacles]]: crates, carts and gates that are gone round, though they were not there when the nav was made
7. [[Portals and Dimensions]]: Portals walked through as a short way, and agents sent into another dimension, as into a dungeon

In a [[Block Worlds|Block World]] the places to walk are worked out from the blocks themselves, chunk by chunk, as the world streams in and as blocks are placed and dug. There is nothing to bake. [[Block Mobs]] walk on the same places.

Every part of it is under **Add Component ▸ Navigation**.

## Seeing it in the Scene view

Choose **Gizmos ▸ Navigation** in the Scene view's header. Every place something can stand near the camera shows as a square on its floor: green on ground, light blue in shallow water, deep blue on water that is swum, and orange in a shut door. Each Nav Agent's and Block Mob's way shows as a yellow line from corner to corner, while editing and while playing, and each Nav Obstacle as a red box, brighter while it is cut out of the nav. Where there is no square, nothing walks.

## From a terminal or an AI assistant

`vox editor call` and `vox mcp` reach navigation as they reach the rest of the editor: `navigation.status` says what is kept for each Block World and how many Nav Obstacles there are and carve, `navigation.path` finds a way as an agent would, `navigation.nearest` finds the nearest place to stand, `navigation.agents` lists what every agent is doing, and `nav_agent.set_destination` sends one somewhere. A Nav Agent's settings are set with `component.add` and `field.set`, as every component's are.

## Not done yet

- Climbing vines and ropes without a Nav Link.
- Agent sizes in Project Settings, and costs of ground set per agent.
- Walking from one world of blocks into another where they touch, with no Nav Link between them.
