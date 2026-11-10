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
| Smooth | Whether its surface runs smoothly between its blocks rather than as cubes: a [[Voxel Terrain]] |
| Ground | **Flat**, **Generated** by a world generator (see below), or a **Planet** (see [[Voxel Terrain#Planets|Planets]]) |
| Ground size | How many blocks its ground reaches along X and Z |
| Ground height | With flat ground, how many blocks high it is |
| Top block, Under block, Base block | The kinds of block its ground is made of: its top, the layers under that, and the rest |
| Under depth | How many layers of the under block there are |
| Cast shadows | Whether its blocks cast the sun's shadows |

A change to its ground makes its blocks again at once, what you built on it kept. Each change is one step to undo, and is written to the asset when you save the level.

Its blocks are made from its ground as it loads, the same on every machine. What you built since is kept beside the `.blockworld` in a `.blocks` file of the same name, holding only the chunks you changed, each as its changed blocks one by one while they are few, or whole once that is smaller: a house on a field takes a few hundred bytes. Keep the two together when you move or copy them. A built game ships both.

## Generated ground

With **Ground** set to **Generated**, a [[Generating Terrain|world generator]] makes the ground: the same mountains, coasts and climate a terrain's Generate layer lays, a column of blocks at a time. Each column rises as high as the generator's ground there, its top the top block, the under block below that, and the base block the rest.

| Setting | What it does |
| --- | --- |
| Generator | The `.worldgen` its ground comes from |
| Offset | Where its corner is in the generator's world, in metres |
| Lowest block | How far down its ground goes, in blocks from its corner: below that is air |
| Overhangs | How far its surface is pushed in and out, in metres, so cliffs lean and overhang |
| Caves | How much of the ground below is hollowed out, from 0 for none to 1: long tunnels, and chambers here and there |
| Sea block | The kind of block the air below the generator's sea level is filled with, a kind of [[Block Worlds#Water|water]]; Air for none |

Caves keep three blocks under the surface whole, so they open onto it only where a slope cuts into them. The sea fills only the open air over the ground: caves under it stay dry.

A kind of block can **cover** generated ground in place of its top block, by the ground's climate and height. Under **Build**, tick a kind's **Cover**, and its rows show:

| Setting | What it does |
| --- | --- |
| Biome | Sets its temperatures and humidities to a biome's at once (see [[Surfaces#Biomes|Biomes]]) |
| Temperature, Temperatures | Covers only between two temperatures, in °C |
| Humidity, Humidities | Covers only between two humidities, from 0 for dry to 1 for wet |
| Height, Heights | Covers only between two heights in the generator's world, in metres |
| Depth | How many blocks deep it lies |

Snow covering where it is below freezing, sand where it is hot and dry and low, gravel on the tops: later kinds in the list cover those before them. A change to the generator makes the ground again, with what you built on it kept.

## Without end

Generated ground can go on **without end**: tick **Endless** under its generator's settings, and its ground is made round the camera as it goes, from its generator, a column of chunks at a time on threads of their own, the nearest first.

| Setting | What it does |
| --- | --- |
| View distance | How far round the camera its own blocks are made, in metres: built on, stood on and drawn |
| Far distance | How far coarser blocks are drawn beyond, in metres |

Beyond its view distance, it is drawn in rings of coarser blocks, each ring's blocks twice as large as the one inside it, so mountains stand on the horizon at a fraction of the cost. A coarse ring shows the ground until the finer one inside it is made, and never where it is. Only its own blocks near the camera can be built on and stood on.

What you build is kept as it goes out of reach and made again as it comes back, and saving keeps it all, near or away, in its `.blocks` file. In a game it is made round the camera; in the editor, round where the Scene view looks from.

However far the camera goes, positions stay exact: the world's middle moves along with it (see [[Floating Origin]]), and the chunks already made stay as they were.

## Kinds of block

Under **Build**, the world's kinds of block are listed, numbered from 1 in the order they come. A click picks the one the brush builds with, and its settings show below the list:

| Setting | What it does |
| --- | --- |
| Name | What it is called |
| Water | Whether it is [[Block Worlds#Water|water]] |
| Color | Its colour, where a face has no material; for water, the water's own |
| Material | The material every face is drawn with, but its top and bottom where they have their own |
| Top | Its top's material |
| Bottom | Its bottom's material |

A face with a material is drawn with it, its pictures repeated a block at a time and taken from the side the face looks, so stripes on a side run straight down it. A face with none is drawn in the block's colour.

**Add Block** adds a kind of a colour of its own at the end. Drag a kind by its grip to move it, and its cross takes it out: its blocks with it, and the kinds after it a number up, every block renumbered to match. Each change to the list is one step to undo.

## Water

Tick a kind's **Water** and its blocks are water, drawn see-through with the same look as the sea and Pools. Its **Color** is the water's own colour, the colour it shows where it is deep: lake green, sea blue or a muddy brown. Water has no materials, so its Material, Top and Bottom rows go. Build with it as with any kind.

Water shows its faces only where it meets air: a pond in a hollow is its top alone. The ground under it and round it shows through it, bent as light bends going into water, and dimmed by as much water as the view goes through: a shallow edge is clear, a deep middle shows its colour. Its top ripples, catching the sky and the sun, with a line of foam where it meets the shore. From under the water, its top shows the world above it.

Nothing stands on water. Physics leaves it out, so things fall through it to the ground under it, and a Rigidbody with **Buoyancy** floats in it as it does on a Pool: the top of the water in its column is the surface.

### Flowing water

Water flows. Each block of water you place is a **source**: it stays full and never runs dry. Water runs out of it, down first and then out over the ground, an eighth of a block lower for each block it goes, for seven blocks at most. Over an edge it falls as a column to the ground below and runs on from there. Water runs out only over ground, so a waterfall keeps to its column, and water falling into a lake joins it. Take a source away and the water it fed dries up behind it. Dig a trench beside a pond and the pond runs into it.

Water flows five blocks a second, the same on every machine, both in play and in the editor as you build; while a game is paused, it stands still. It starts wherever something changes, so a generated sea stays as it was made until you dig into it or build beside it. It stays within the chunks its world reaches: at the world's edge it stops as if at a wall. Saving keeps the water where it has run to.

On a planet, down is towards its middle. Water falls towards the middle and runs out across the planet's surface, whichever side it is on.

For a sea, pick a kind of water as the **Sea block** of generated ground: the air below the generator's sea level is filled with it. A planet's sea lies its **Radius** from its middle.

In a [[Voxel Terrain]], water stays cubes: its top is flat at its blocks' tops, with the smooth ground running on under it. Its sides show wherever it meets anything but water, so a shore where the ground dips below the water's edge still shows the water there. It flows there as it does among cubes.

Not done yet: with the camera in water blocks, the view is not fogged by the water round it, as it is under the sea and in Pools. On a planet, Buoyancy floats things only on its top, where the planet's up is the world's.

## Building

With **Build** showing, the brush works in the Scene view over the selected world's blocks, its outline round the blocks it would change.

| Setting | What it does |
| --- | --- |
| Brush | **Place** puts the picked kind against the face under the pointer; **Break** takes the block there away; **Paint** turns it into the picked kind; **Model** builds a model of blocks (see below) |
| Size | How many blocks across it reaches: 1 for a block at a time, more for a ball of them |
| Model | With the Model brush, the `.vox` model it builds |

Click to build; drag to go on block by block. Hold **Shift** to break while placing, and to place while breaking. Each stroke is one step to undo.

## Models of small blocks

A **Block Model** is a `.vox` file, as voxel sculpting tools save them: a model of coloured blocks, or a scene of several placed together. Put one in the project like any other asset. With the brush on **Model**, its **Model** row picks one, its outline shows where it would stand, and a click builds it standing on the face under the pointer, its middle over the block clicked. Up in the file is up in the world, and each model of a scene stands where the scene puts it.

Each of its colours builds with a kind of block of that colour: one the world has already, of that colour and with no material, or a new one added at the end of the list. A world has room for 255 kinds; once the list is full, a colour builds with the nearest one there is. A click builds the model once, and is one step to undo, the kinds it added with it.

For a world of small coloured blocks to sculpt with, make **Block size** small: at 0.1 m, a model 32 blocks high stands about three metres tall.

## How blocks are drawn

The blocks are kept in chunks of 32 a side, only the chunks with blocks in them, so a world is as large as its blocks and no larger. A face is drawn only where a block meets air or water, and faces side by side drawn the same way are drawn as one, so a field of grass is a handful of quads. Each corner of a face is a little darker as the blocks round it close it in, which gives edges and hollows their depth.

The camera draws only the chunks in view, and of each only the faces turned towards it. Caves and rooms closed off from it are passed over too: each chunk notes which of its sides see each other through its air and water, and only the chunks the camera can see into through them are drawn, so a mountain full of caves costs no more than its outside until a tunnel opens onto one. The sun's shadows fall from every face. Blocks light the world round them too, as the GI's rays see them.

Water's faces are drawn after the rest of the scene, with the sea and Pools: every face of the water in the chunks the camera sees, from either side, flowing water as high as it stands. Water casts no shadow, and the GI's rays pass through it.

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

A ray from **Blocks.Raycast** meets water as it meets any block, so a script can take water away or build on it. To build on the ground under a pond, take the water away first.

Physics and drawing follow a change at once: only the chunks it changed are made again, and those beside them whose faces it meets. A save of the scene keeps what scripts changed, as only what differs from the world as its level built it, and `Blocks.CaptureEdits` and `Blocks.ApplyEdits` give the same bytes to a player joining a game.

## Collision

A world of blocks collides as it is drawn: physics stands things on its blocks, a mesh for each chunk, and rays from **Physics.Raycast** meet them, its Obj as what they hit. It needs no Collider. Its Obj's **Layer** is its blocks' layer. Its water does not collide: things fall into it, and those with Buoyancy float.
