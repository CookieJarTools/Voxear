---
order: 5
---

A level made of models rather than blocks has its navigation **baked**: worked out once from its colliders, saved beside the level, and read as the level opens. Nav Agents and Block Mobs walk it as they walk a Block World.

## Baking

1. Give the floors, walls, stairs and ramps of your level **Colliders**. Only still colliders count: not triggers, and not those with a Rigidbody.
2. Open the level, select its `.level` file in the Project panel, and choose **Bake Navigation** under **Navigation** in the Inspector. **Bake Navigation** in the command palette does the same.
3. The status bar says **Baking navigation** while it works. When it is done, a `.nav` file stands beside the level, and agents walk on it at once.

Bake again whenever you move a collider. The `.nav` file goes into your builds with the level.

## What it walks

- Floors no steeper than 45 degrees, with room over them for something 1.8 metres tall.
- Steps up to 45 centimetres, so stairs and ramps join one floor to the next.
- Never nearer a wall or an edge than 35 centimetres, so a body that wide keeps off it.
- Floors over floors stay apart: a way on the ground passes under a balcony, and reaches it only by stairs.

Turn on **Gizmos ▸ Navigation** to see what was baked: a green tint on every floor it walks, in rectangles that follow the floor. Where there is no tint, nothing walks.

## From a terminal or an AI assistant

`navigation.bake` bakes the open level and answers with the file and how many polygons it has. `navigation.path` then finds ways over it, as it does over a Block World.

## Not done yet

- Setting what is baked for, such as a smaller creature or a steeper slope.
- Mending the mesh where an Obj moves, without baking again.
