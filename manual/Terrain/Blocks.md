---
order: 19
---

A **Block** is one kind of block: a `.block` asset with how it looks and its rules. Every block in the project is there for every [[Block Worlds|Block World]] and [[Voxel Terrain]] to hold: each draws it the same, and a change to the block reaches them all.

## The Blocks panel

**Window ▸ Blocks** opens the Blocks panel: every block of the project, each by its picture and its name. Type in its search box to narrow them, by name or by folder.

| Action | What it does |
| --- | --- |
| Click | Picks a block, and the Inspector shows it |
| Double-click | Has the Scene view's brush build with it |
| Drag | Onto a Block World's **Block**, or any field that takes a block |
| New Block | Makes a block of a colour of its own in the project's **Blocks** folder |
| Duplicate | Copies the picked block beside it |
| Delete | Moves the picked block to the project's trash, `.voxear/Trash` |

While worlds hold a block, **Delete** first says which, and offers **Delete Anyway**. A block deleted so is drawn bright pink in the worlds that hold it, until you put its file back.

A block is called what its file is. Rename it in the Project panel, where **New Block** in a folder's menu makes one too, and **New Glass Block** a block of glass whose panes join (see [Faces that join](#faces-that-join)).

## A block's settings

Pick a block, and the Inspector shows:

### Look

| Setting | What it does |
| --- | --- |
| Color | Its colour, where a face has no material; a liquid's own colour |
| Material | The material every face is drawn with, but its top and bottom where they have their own |
| Top | Its top's material |
| Bottom | Its bottom's material |
| Sways in wind | Its faces' corners move with the level's wind, as leaves do (see [[Wind#Blocks that sway]]) |
| Liquid | **None**, or **Water**: see-through, flowing, and nothing stands on it (see [[Block Worlds#Water|Water]]) |

A face with a material is drawn with it, its pictures repeated a block at a time and taken from the side the face looks, so stripes on a side run straight down it. A face with none is drawn in the block's colour, matt: it shines back nothing of the sky. For a block that shines, such as ice or polished stone, give it a material with a low **Roughness**. A liquid is drawn in its colour alone, so its materials go.

### Leaves and other see-through blocks

A block drawn with a material that is **cut out** is see-through: leaves, a grating, a web. Give the material a colour map with clear parts and a `cutout` (see [[Materials]]), and use it on the block. Nothing else needs setting.

- Its faces are drawn only where the picture is solid, and what is behind shows through the rest.
- Blocks beside it keep their faces towards it, so a trunk shows through its leaves.
- It keeps its own faces towards other see-through blocks, its own kind too, so a crown of leaves is leaves all the way in, darker inside.
- The sun passes where the picture is clear, so its shadow is its picture's: dappled under a crown of leaves.
- It is a block in every other way: stood on, built on, dug, and it shades the corners of the blocks round it.

If any of a block's materials is cut out (its sides', its top's or its bottom's), the whole block is see-through. Far off, in the coarser rings, it is drawn like any block. Bounced light takes it as solid, and a see-through block is lit as any block is: light does not glow through a leaf from behind.

### Faces that join

A block drawn with a material that has **Connected tiles** joins those of its own kind beside it: a wall of glass is one sheet with a frame round the whole of it, not a frame round every pane, and a floor of slabs has one border round its edge.

**New Glass Block** in a folder's menu makes one to start from: a block, its material and its picture, side by side. To make your own, give the block's material a colour map of five tiles side by side, each as wide as it is high, and turn on **Connected tiles** (see [[Materials#Pictures for blocks]]):

| Tile | What it shows |
| --- | --- |
| 1 | The face alone: its border all round |
| 2 | Filled: no border |
| 3 | Joined left and right: its border along its top and bottom |
| 4 | Joined up and down: its border down its sides |
| 5 | Joined all round: only its inner corners |

Draw the same inside in all five, and only the border differently. Each quarter of a face is taken from the tile the three faces round that quarter make it, so five tiles cover every shape a group of blocks can have.

- A face joins the face beside it when that is the same block and shows too. A block in front of the one beside it breaks the join, so a border runs along where a wall meets it.
- No face is drawn between two of the block, see-through or not: glass is one body, with nothing inside it.
- Faces join by their own material: a block whose sides join and whose top does not is a bookcase, or a row of crates.
- For glass, give the material a `cutout` and a picture that is clear inside its frame. The sun passes the panes and the frame casts its shadow.
- A [[Texture Packs|texture pack]] can make a block join, or stop it joining, with the pictures it brings.
- A joining face is drawn plain in a world of [[Block Worlds#Faces of small cubes|small cubes]].
- A world drawn smooth joins nothing: give its blocks pictures of one tile.

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

Each change is written to the block's file as you make it, and every world holding the block is drawn as it now says at once. There is nothing to save, and nothing to undo.

## Other pictures for blocks

Whoever plays your game can pick other pictures for its blocks: a [[Texture Packs|texture pack]] names a block and the pictures its top, sides and bottom are drawn with in its place. A block of a colour alone can be given a picture so, and a block with a material another.

## Blocks in a build

Every block of the project ships, with the materials, items and sounds it names, so a game can place any of them and `Blocks.All` is the same list as in the editor. To leave some out, name them under `exclude` in the project's content settings; one a world holds or an item places ships all the same. A package's block ships when something the game ships uses it.
