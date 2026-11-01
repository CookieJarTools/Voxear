---
order: 4
---

Under the Objs you see in the Hierarchy, the engine keeps everything in the world as **entities**: plain records that each hold a set of **components**, pieces of data such as a Transform. Every Obj is an entity, and your C# code can make entities that are not Objs, for things there are thousands of, such as bullets or grass.

The Entities panel shows the world as the engine sees it. It is in the **Profiling** layout, beside the Profiler.

## The columns

- **Archetypes**: entities with exactly the same components are kept together, in a group called an archetype. Each row shows how many entities it holds and which components they have, the busiest first.
- **Entities**: the entities in the archetype you picked, by name. Picking one that is an Obj selects it in the Hierarchy and the Inspector.
- **Components**: the picked entity's components, with their values.
- **Systems**: the code that runs over entities each frame, in the order it runs, with how long each took in the frame the Profiler is showing.

Type component names in the box at the top, separated by commas, to see only the archetypes that have them all: `Transform, Spin`.

## Changing values

You can change the values of entities that are not Objs right here, while the game runs, to try things out. Change Objs in the Inspector instead, where changes can be undone and saved.

The panel reads the world again a few times a second while it is showing.
