---
order: 10
---

A **Walkable** on an Obj with a still collider joins it to the navigation of a world of blocks: its floors and stairs are stood on, its walls are gone round, and its doorways lead in from the ground outside. A house from a kit is walked into and through as its blocks would be, with nothing baked by hand.

## Making a house walkable

1. Give the house's Obj a **Collider** with **Shape** set to **Mesh** (or boxes for its walls and floors), and no Rigidbody.
2. Choose **Add Component ▸ Navigation ▸ Walkable**.

Put it on a prefab's root and every instance is walkable. Its cells are worked out again whenever the Obj is placed or moved, in the editor and while the game plays, and only the chunks it left or came to are made again.

Turn on **Gizmos ▸ Navigation** to see the places to stand on its floors.

## Doors

A door or a gate that swings open is walked through, not round. Give its own Obj a collider and a Walkable with **Door** on: it is left out of the navigation, so the doorway it hangs in stays open to walk through, wherever the door stands.

## How it works

Every surface of the collider is laid into the world's blocks, a block for each cell it passes through, for the navigation alone: nothing is drawn or saved. So a floor is a layer of cells to stand on, a wall a column to go round, and a step a block high is climbed as a block is. Gaps narrower than a block are closed, and slopes gentler than a block a block are steps.

`vox editor call navigation.status` counts the Walkables and, for each world, how many cells its models stand in.

### From scripts

`Obj.Walkable` is an Obj's Walkable, null when it has none, and `Obj.AddWalkable` gives it one, with `Door`.
