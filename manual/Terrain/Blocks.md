---
order: 19
---

A **Block** is one kind of block: a `.block` asset with how it looks and its rules. [[Block Worlds]] and [[Voxel Terrain|Voxel Terrains]] list the blocks they are made of, so every world listing a block draws it the same, and a change to the block reaches them all.

## Making a block

Right-click in the Project panel and choose **New Block**, or use **New Block** under a Block World's **Build** tool, which lists it in that world too. A block is called what its file is.

Select it, and the Inspector shows:

### Look

| Setting | What it does |
| --- | --- |
| Color | Its colour, where a face has no material; a liquid's own colour |
| Material | The material every face is drawn with, but its top and bottom where they have their own |
| Top | Its top's material |
| Bottom | Its bottom's material |
| Liquid | **None**, or **Water**: see-through, flowing, and nothing stands on it (see [[Block Worlds#Water|Water]]) |

A face with a material is drawn with it, its pictures repeated a block at a time and taken from the side the face looks, so stripes on a side run straight down it. A face with none is drawn in the block's colour. A liquid is drawn in its colour alone, so its materials go.

### Breaking

| Setting | What it does |
| --- | --- |
| Hardness | How many seconds it takes to break by hand |
| Unbreakable | Nothing breaks it |
| Tool | The [[Tool Kinds and Stations|kind of tool]] that breaks it fast |
| Tier | The lowest tier of that tool that gets its drops; 0 for anything, a hand too |

With a tool of its kind in hand, a block takes its Hardness divided by the tool's **Speed**. Any other tool is no better than a hand. A block with a Tier still breaks for a weaker tool, but drops nothing.

### Drops

What it drops when it is broken: each an **Item**, how many of it, from the fewest to the most, and its **Chance** from 0 to 1. A block with no drops drops nothing.

**Make Item** shows while no item places the block. It makes the [[Items|item]] the block is carried as, in the project's **Items** folder, named as the block is, and a block that drops nothing yet drops it.

### Sounds and Data

**Break**, **Place** and **Step** are the sounds of breaking it, putting it down and walking on it. **Data** is a [[Data Assets|data asset]] of your own for it: whatever your scripts want to know of the block, or rules of its own (see [[Blocks and Items from Scripts#Rules of your own|Rules of your own]]).

Each change is written to the block's file as you make it, and every world listing the block is drawn as it now says at once. There is nothing to save, and nothing to undo.

## Blocks in a build

A block ships with a world that lists it, or an item that places it, with the materials, items and sounds it names. A block only code loads by its path, with `Block.Load`, needs a `[ContentRoot]` as any asset loaded that way does (see [[Data Assets]]).
