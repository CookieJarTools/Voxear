---
order: 17
---

A **Block World** is ground made of blocks: a cube each, of the kinds its world has, grass, dirt, stone or anything you make, from big blocks to build with down to small ones to sculpt with. It reaches from its Obj's position along +X, +Y and +Z, and only the Obj's position places it.

## Making a world of blocks

Select an Obj and pick **Add Component ▸ Terrain ▸ Block World**. It starts as flat ground 64 blocks a side and 8 up, grass over three layers of dirt over stone, on a new asset of its own in the project's **Blocks** folder, named for its Obj: `Blocks/Ground.blockworld`. Every Block World that picks the same asset has the same blocks.

Along the top of the Block World in the Inspector are its tools: **Build**, its kinds of block and the brush that builds with them, and **Settings**:

| Setting | What it does |
| --- | --- |
| Data | The `.blockworld` asset its blocks are |
| Block size | How many metres a block is a side |
| Ground size | How many blocks its ground reaches along X and Z |
| Ground height | How many blocks high its ground is |
| Cast shadows | Whether its blocks cast the sun's shadows |

A change to its ground makes its blocks again at once, what you built on it kept. Each change is one step to undo, and is written to the asset when you save the level.

Its blocks are made from its ground as it loads, the same on every machine. What you built since is kept beside the `.blockworld` in a `.blocks` file of the same name, holding only the chunks you changed, each as its changed blocks one by one while they are few, or whole once that is smaller: a house on a field takes a few hundred bytes. Keep the two together when you move or copy them. A built game ships both.

## Kinds of block

Under **Build**, the world's kinds of block are listed, numbered from 1 in the order they come. A click picks the one the brush builds with, and its settings show below the list:

| Setting | What it does |
| --- | --- |
| Name | What it is called |
| Color | Its colour, where a face has no material |
| Material | The material every face is drawn with, but its top and bottom where they have their own |
| Top | Its top's material |
| Bottom | Its bottom's material |

A face with a material is drawn with it, its pictures repeated a block at a time and taken from the side the face looks, so stripes on a side run straight down it. A face with none is drawn in the block's colour.

**Add Block** adds a kind of a colour of its own at the end. Drag a kind by its grip to move it, and its cross takes it out: its blocks with it, and the kinds after it a number up, every block renumbered to match. Each change to the list is one step to undo.

## Building

With **Build** showing, the brush works in the Scene view over the selected world's blocks, its outline round the blocks it would change.

| Setting | What it does |
| --- | --- |
| Brush | **Place** puts the picked kind against the face under the pointer; **Break** takes the block there away; **Paint** turns it into the picked kind |
| Size | How many blocks across it reaches: 1 for a block at a time, more for a ball of them |

Click to build; drag to go on block by block. Hold **Shift** to break while placing, and to place while breaking. Each stroke is one step to undo.

## How blocks are drawn

The blocks are kept in chunks of 32 a side, only the chunks with blocks in them, so a world is as large as its blocks and no larger. A face is drawn only where a block meets air, and faces side by side drawn the same way are drawn as one, so a field of grass is a handful of quads. Each corner of a face is a little darker as the blocks round it close it in, which gives edges and hollows their depth.

The camera draws only the chunks in view, and of each only the faces turned towards it. The sun's shadows fall from every face. Blocks light the world round them too, as the GI's rays see them.

## Blocks from scripts

**Blocks** in a script asks which block is where and changes them, in every Block World a change reaches. A kind of block is its number in its world's list; 0 is air.

```csharp
// Break the block the player looks at, or put stone against it.
if (Blocks.Raycast(eye.Position, eye.Forward, out var hit, 8f))
{
    if (fire) Blocks.Break(hit);
    if (use) Blocks.Place(hit, 3);
}

// A shell's blast: every block within 2 m gone.
Blocks.Break(impact, 2f);
```

| Call | What it does |
| --- | --- |
| `Blocks.At(position)` | The block at a point; 0 for air |
| `Blocks.Raycast(origin, direction, out hit, maxDistance)` | The first block a ray meets: its Obj, point, face, the block's middle and kind |
| `Blocks.Set(position, block)` | Sets one block |
| `Blocks.Fill(from, to, block)` | Sets every block in a box |
| `Blocks.Ball(middle, radius, block)` | Sets every block in a ball |
| `Blocks.Break(middle, radius)` | Takes away every block in a ball |
| `Blocks.Place(hit, block)` and `Blocks.Break(hit)` | Puts a block against the face a ray met, or takes away the block it met |

Physics and drawing follow a change at once: only the chunks it changed are made again, and those beside them whose faces it meets. A save of the scene keeps what scripts changed, as only what differs from the world as its level built it, and `Blocks.CaptureEdits` and `Blocks.ApplyEdits` give the same bytes to a player joining a game.

## Collision

A world of blocks collides as it is drawn: physics stands things on its blocks, a mesh for each chunk, and rays from **Physics.Raycast** meet them, its Obj as what they hit. It needs no Collider. Its Obj's **Layer** is its blocks' layer.
