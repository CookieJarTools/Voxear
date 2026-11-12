---
order: 17
---

A **Block World** is ground made of blocks: a cube each, of the [[Blocks|blocks]] its world lists, grass, dirt, stone or anything you make, from big blocks to build with down to small ones to sculpt with. It reaches from its Obj's position along +X, +Y and +Z, and only the Obj's position places it.

## Making a world of blocks

Select an Obj and pick **Add Component ▸ Terrain ▸ Block World**. It starts as flat ground 64 blocks a side and 8 up, grass over three layers of dirt over stone, on a new asset of its own in the project's **Blocks** folder, named for its Obj: `Blocks/Ground.blockworld`. Its grass, dirt and stone are the project's blocks of those names, `Blocks/Grass.block` and the rest, made the first time a world wants them. Every Block World that picks the same asset has the same blocks.

Along the top of the Block World in the Inspector are its tools: **Build**, the blocks it lists and the brush that builds with them, and **Settings**:

| Setting | What it does |
| --- | --- |
| Data | The `.blockworld` asset its blocks are |
| Block size | How many metres a block is a side, from 0.1 to 10: smaller blocks make the same world finer, not smaller (see below) |
| Smooth | Whether its surface runs smoothly between its blocks rather than as cubes: a [[Voxel Terrain]] |
| Ground | **Flat**, or **Generated** by a world generator (see below) |
| Ground size | How many blocks its ground reaches along X and Z |
| Ground height | With flat ground, how many blocks high it is |
| Top block, Under block, Base block | The blocks its ground is made of, of those it lists: its top, the layers under that, and the rest |
| Under depth | How many layers of the under block there are |
| Cast shadows | Whether its blocks cast the sun's shadows |

A change to its ground makes its blocks again at once, what you built on it kept. Each change is one step to undo, and is written to the asset when you save the level.

Changing **Block size** keeps the world as it is in metres and makes it again from blocks of the new size. Its ground stays as wide, as high and as deep, so the rows counted in blocks (Ground size, Ground height, Under depth, Lowest block, a kind's cover Depth) are counted again: halve the block size and each doubles. What you built stays where it was: a wall a metre thick is two blocks thick at half the size, and in a Voxel Terrain the ground's surface stays where it lay. Going to larger blocks loses what was finer than they are. A block is 0.1 m a side at the smallest and 10 m at the largest, and flat ground is at most 4096 blocks a side, which can keep its blocks larger still. Dragging a number by its label, as Block size or Ground size, shows the number as you drag and makes the world again once you let go. A large world, such as one of small blocks, is made again in the background: the Inspector shows the new numbers at once, and the Scene view shows the world as it was until the new one is ready, after a second or two. Undo takes it back in one step; undoing before it is ready lets the change go.

Its blocks are made from its ground as it loads, the same on every machine. What you built since is kept beside the `.blockworld` in a `.blocks` file of the same name, holding only the chunks you changed, each as its changed blocks one by one while they are few, or whole once that is smaller: a house on a field takes a few hundred bytes. Keep the two together when you move or copy them. A built game ships both.

## Generated ground

With **Ground** set to **Generated**, a [[Generating Terrain|world generator]] makes the ground: the same mountains, coasts and climate a terrain's Generate layer lays, a column of blocks at a time. Each column rises as high as the generator's ground there, its top the top block, the under block below that, and the base block the rest.

| Setting | What it does |
| --- | --- |
| Generator | The `.worldgen` its ground comes from |
| Offset | Where its corner is in the generator's world, in metres |
| Lowest block | How far down its ground goes, in blocks from its corner: below that is air |
| Overhangs | How far its steep ground is pushed in and out, in metres, so cliffs lean, bulge and overhang |
| Caves | How much of the ground below is hollowed out, from 0 for none to 1: long tunnels, and chambers here and there |
| Sea block | The block the air below the generator's sea level is filled with, one that is [[Block Worlds#Water|water]]; Air for none |

Overhangs push cliffs and leave hillsides alone: ground steeper than about 30° is pushed a little, and ground steeper than about 45° by the whole amount. The bulges are about 14 m across, so the ground hangs over open air once Overhangs is 9 m or so; under that, cliffs lean and bulge. The far rings of a world without end, whose blocks are too large to hold a bulge, are left unpushed.

The ground's layers follow the pushed ground. The top block is whatever has open air over it, with the under block below, counted down each column, so the under block never shows through where the ground was pushed out. Under an overhang the count starts again on the ground below, and the overhang's underside has no top block.

Caves keep three blocks under the surface whole, so they open onto it only where a slope cuts into them. The sea fills only the open air over the ground: caves under it stay dry.

A block the world lists can **cover** generated ground in place of its top block, by the ground's climate and height. Under **Build**, pick the block and tick its **Cover**, and its rows show:

| Setting | What it does |
| --- | --- |
| Biome | Sets its temperatures and humidities to a biome's at once (see [[Surfaces#Biomes|Biomes]]) |
| Temperature, Temperatures | Covers only between two temperatures, in °C |
| Humidity, Humidities | Covers only between two humidities, from 0 for dry to 1 for wet |
| Height, Heights | Covers only between two heights in the generator's world, in metres |
| Depth | How many blocks deep it lies |

Snow covering where it is below freezing, sand where it is hot and dry and low, gravel on the tops: blocks later in the list cover those before them. A cover is the world's, not the block's: the same snow can cover one world's peaks and none of another's. A change to the generator makes the ground again, with what you built on it kept.

### Overhangs and caves in the generator

Overhangs and Caves can belong to the generator rather than the world, so every world on that generator shares them. **Move to Generator**, above the settings, moves the world's two numbers into its generator. The generator's graph gains the nodes that make them, and the generator shows three rows of its own:

| Row | What it does |
| --- | --- |
| Overhangs | How far steep ground is pushed in and out, in metres |
| Tunnels | How wide the long winding caves are, from 0 for none to 1 |
| Chambers | How much of the ground the wide low caves take, from 0 for none to 1 |

The world's own Overhangs and Caves rows go, since the generator now makes both, for this world and any other on the generator. The ground of a world whose **Offset** is 0, 0 stays exactly as it was. A world placed elsewhere takes the caves that lie there in the generator's world, so its caves move.

The nodes are wired into the **Volume** of the graph's Output, which is the ground in three dimensions: how far into the ground each place is. A terrain of heights reads the Output's height and never its Volume.

| Node | What it makes |
| --- | --- |
| Solid | A height made solid: the generator's own ground with nothing wired in |
| Overhangs | Ground pushed in and out by **Amount** metres, in bulges **Size** metres across with **Octaves** finer ones over them. It pushes where its **Mask** says, and steep ground alone with no mask wired in |
| Tunnels | Long winding caves, **Amount** wide and winding over **Size** metres, kept **Roof** metres under the ground |
| Chambers | Wide low caves **Size** metres apart, taking **Amount** of the ground, kept **Roof** metres under it |
| Cave Network | Rooms you make as prefabs, joined by winding tunnels, without end or as one bounded level. [[Caves]] is about them |
| Noise 3D | Noise through space, from **Low** to **High** |
| Combine | Two of these made one: **Union**, **Subtract** or **Smooth**, which runs them together over **Blend** metres |

What a Subtract takes away counts as caves: the far rings are made without it, the sea does not flood it, and the ground's layers run through it. A Volume is read up to 64 m over the generator's ground and no higher.

## Without end

Generated ground can go on **without end**: tick **Endless** under its generator's settings, and its ground is made round the camera as it goes, from its generator, a column of chunks at a time on threads of their own, the nearest first.

| Setting | What it does |
| --- | --- |
| View distance | How far round the camera its own blocks are made, in metres: built on, stood on and drawn |
| Far distance | How far coarser blocks are drawn beyond, in metres |

Beyond its view distance, it is drawn in rings of coarser blocks, each ring's blocks twice as large as the one inside it, so mountains stand on the horizon at a fraction of the cost. A coarse ring shows the ground until the finer one inside it is drawn, and never where it is. Only its own blocks near the camera can be built on and stood on.

Its own blocks are made a column of chunks further out than they are drawn, and each column's faces are made on those threads too, once the columns round it have their blocks. So every column is made once, its edges meeting the ground beside it, and the game never waits for one.

A [[Block Worlds#Water|sea]] runs on across the rings unbroken: it stands as high in every ring, and its water shows no sides where they meet, nor at the edge of what is made, where the world goes on.

What you build is kept as it goes out of reach and made again as it comes back, and saving keeps it all, near or away, in its `.blocks` file. In a game it is made round the camera; in the editor, round where the Scene view looks from.

With the [[Floating Origin]] switched on, positions stay exact however far the camera goes: the world's middle moves along with it, and the chunks already made stay as they were.

## The world's blocks

Under **Build**, the blocks the world is made of are listed, each with its colour and its number. A click picks the one the brush builds with.

Most are [[Blocks|blocks]]: `.block` assets, each with its look and its rules in a file of its own, which every world listing it shares.

| Button | What it does |
| --- | --- |
| Add Block | Lists a block of the project, picked from them all. A `.block` dragged onto the list from the Project panel is listed too |
| New Block | Makes a block of a colour of its own in the project's **Blocks** folder, and lists it |
| Add Color | Adds a kind of the world's own: a name and a colour, kept in the world's file. Models of small blocks bring their colours this way |
| Make Block Assets | Shows while the world keeps kinds of its own that are more than a model's colours, as worlds made before blocks were assets do: makes a block of each, and lists it in the kind's place |

Under the list, the picked block shows:

| Setting | What it does |
| --- | --- |
| Block | The `.block` it is. Pick another and every block of its number is that one. Double-click it to open the block itself, for its rules |
| Water | Whether it is [[Block Worlds#Water|water]] |
| Color | Its colour, where a face has no material; for water, the water's own |
| Material, Top, Bottom | The materials its faces are drawn with |
| Cover | With generated ground, where it covers the ground in place of its top block |

A block's water, colour and materials are the block's own. Changed here, they are written to its file at once and change it in every world that lists it, and they are no step to undo. A kind of the world's own has a **Name** in place of Block, and its changes are the world's, undone like any other.

### A block's number never changes

A block's number is its place in the world's list, from 1, and what the world's blocks are kept by. A block's cross takes it out of the list, its blocks with it. Its place stays, empty, so every other block keeps its number, and the next block added takes a new one. The list has no order to change. Each change to the list is one step to undo.

A world made before blocks were assets keeps loading exactly as it was, kinds and all, until you choose **Make Block Assets**. Nothing built changes when you do: each block keeps its number.

## Water

A block whose **Liquid** is **Water** is water, drawn see-through with the same look as the sea and Pools. Its **Color** is the water's own colour, the colour it shows where it is deep: lake green, sea blue or a muddy brown. Water has no materials, so its Material, Top and Bottom rows go. Build with it as with any block.

Water shows its faces only where it meets air: a pond in a hollow is its top alone. The ground under it and round it shows through it, bent as light bends going into water, and dimmed by as much water as the view goes through: a shallow edge is clear, a deep middle shows its colour. Its top ripples, catching the sky and the sun, with a line of foam where it meets the shore. From under the water, its top shows the world above it.

Nothing stands on water. Physics leaves it out, so things fall through it to the ground under it, and a Rigidbody with **Buoyancy** floats in it as it does on a Pool: the top of the water in its column is the surface.

### Flowing water

Water flows. Each block of water you place is a **source**: it stays full and never runs dry. Water runs out of it, down first and then out over the ground or over still water, an eighth of a block lower for each block it goes, for seven blocks at most. Over an edge it falls as a column to the ground below and runs on from there. Falling water runs out only once it lands on ground, so a waterfall keeps to its column, and water falling into a lake joins it. A block with a source on each of two sides becomes a source itself, so a gap between two pools fills to the brim. Take a source away and the water it fed dries up behind it. Dig into the bank of a pond or the sea and the water runs into the hole.

Water flows five blocks a second, the same on every machine, both in play and in the editor as you build; while a game is paused, it stands still. It starts wherever something changes, so a generated sea stays as it was made until you dig into it or build beside it. It stays within the chunks its world reaches: at the world's edge it stops as if at a wall. Saving keeps the water where it has run to.

For a sea, pick a block of water as the **Sea block** of generated ground: the air below the generator's sea level is filled with it.

In a [[Voxel Terrain]], water is smooth too: its surface runs between its blocks as the ground's does. A full block's surface is its top, and flowing water stands as high as it flows. A generated sea lies flat at its generator's sea level; it lies over the ground wherever the ground is under it, however shallow, and its surface runs on a block into the shore, so the waterline is where the ground rises through it, with no gaps. It flows as it does among cubes.

With the camera in the water, the view is the same as under a Pool: the water's own colour thickening with distance, light fading with depth, the surface from below, a waterline across the lens as you come up. The water round the camera is found from the top of its column, as far as that top runs along X and Z, so the ground beside a pond stays dry to look at.

## Building

With **Build** showing, the brush works in the Scene view over the selected world's blocks, its outline round the blocks it would change.

| Setting | What it does |
| --- | --- |
| Brush | **Place** puts the picked block against the face under the pointer; **Break** takes the block there away; **Paint** turns it into the picked block; **Model** builds a model of blocks (see below) |
| Size | How many blocks across it reaches: 1 for a block at a time, more for a ball of them |
| Model | With the Model brush, the `.vox` model it builds |

Click to build once; drag to go on, the brush working again as the pointer moves on by half its size. A brush that builds reaches through water to the ground under it, and what it places takes the water's place; with water itself picked, or breaking, it meets the water. Hold **Shift** to break while placing, and to place while breaking. Each stroke is one step to undo. The brushes work while playing too, on the world selected: what you build there goes when play stops, as everything played does.

## Models of small blocks

A **Block Model** is a `.vox` file, as voxel sculpting tools save them: a model of coloured blocks, or a scene of several placed together. Put one in the project like any other asset. With the brush on **Model**, its **Model** row picks one, its outline shows where it would stand, and a click builds it standing on the face under the pointer, its middle over the block clicked. Up in the file is up in the world, and each model of a scene stands where the scene puts it.

Each of its colours builds with a kind of the world's own of that colour: one the world has already, or a new one added at the end of the list. Colours stay the world's, so a model of two hundred colours is not two hundred files. A world draws its faces 256 ways at most, a way for each colour or material; once it has no room for more, a colour builds with the nearest one there is. A click builds the model once, and is one step to undo, the kinds it added with it.

For a world of small coloured blocks to sculpt with, make **Block size** small before you build: at 0.1 m, a model 32 blocks high stands about three metres tall.

## How blocks are drawn

The blocks are kept in chunks of 32 a side, only the chunks with blocks in them, so a world is as large as its blocks and no larger. A face is drawn only where a block meets air or water, and faces side by side drawn the same way are drawn as one, so a field of grass is a handful of quads. Each corner of a face is a little darker as the blocks round it close it in, which gives edges and hollows their depth.

The camera draws only the chunks in view, and of each only the faces turned towards it. Caves and rooms closed off from it are passed over too: each chunk notes which of its sides see each other through its air and water, and only the chunks the camera can see into through them are drawn, so a mountain full of caves costs no more than its outside until a tunnel opens onto one. The sun's shadows fall from every face. Blocks light the world round them too, as the GI's rays see them.

Water's faces are drawn after the rest of the scene, with the sea and Pools: every face of the water in the chunks the camera sees, from either side, flowing water as high as it stands. Water casts no shadow, and the GI's rays pass through it.

## Blocks from scripts

A script reads the rooms of a world's caves with **Caves** (see [[Caves#Caves from scripts|Caves from scripts]]).

**Blocks** in a script asks which block is where and changes them, in every Block World a change reaches. A script names a block by its `Block`, a field picked in the Inspector, which is the same block in every world.

```csharp
public Block Stone;

// Break the block the player looks at, or put stone against it.
if (Blocks.Raycast(eye.Position, eye.Forward, out var hit, 8f))
{
    if (fire) Blocks.Break(hit);
    if (use) Blocks.Place(hit, Stone);
}

// A shell's blast: every block within 2 m gone.
Blocks.Break(impact, 2f);
```

| Call | What it does |
| --- | --- |
| `Blocks.DefinitionAt(position)` | The block at a point; `Block.None` for air |
| `Blocks.Raycast(origin, direction, out hit, maxDistance)` | The first block a ray meets: its Obj, point, face, the block's middle, and `hit.Definition`, the block it is |
| `Blocks.Set(position, block)` | Sets one block |
| `Blocks.Fill(from, to, block)` | Sets every block in a box |
| `Blocks.Ball(middle, radius, block)` | Sets every block in a ball |
| `Blocks.Break(middle, radius)` | Takes away every block in a ball |
| `Blocks.Place(hit, block)` and `Blocks.Break(hit)` | Puts a block against the face a ray met, or takes away the block it met |

A ray passes through water to what lies behind it, as a hand reaching into a pond does, and a block placed where water is takes the water's place. Pass `liquids: true` to **Blocks.Raycast** to meet the water instead, as a bucket does.

A block the world does not list can be placed all the same: the world numbers it then, for as long as the game runs, and never in its file.

Breaking by a block's rules, how long it takes with a tool, what it drops, and items that place blocks are in [[Blocks and Items from Scripts]].

### Blocks by number

Every call also takes a block's number in its world's list, from 1, with 0 for air: `Blocks.At(position)`, `hit.Block`, `Blocks.Set(position, 3)`. A number is one world's own. For a block the world numbered while the game ran, it is this machine's own too, so over the network send the `Block`, or the item that places it, never its number.

Physics and drawing follow a change at once: only the chunks it changed are made again, and those beside them whose faces it meets. A save of the scene keeps what scripts changed, as only what differs from the world as its level built it, and `Blocks.CaptureEdits` and `Blocks.ApplyEdits` give the same bytes to a player joining a game. The bytes say which block each number given in play is, so they read in a game that numbered its blocks another way.

## Collision

A world of blocks collides as it is drawn: physics stands things on its blocks, a mesh for each chunk, and rays from **Physics.Raycast** meet them, its Obj as what they hit. It needs no Collider. Its Obj's **Layer** is its blocks' layer. Its water does not collide: things fall into it, and those with Buoyancy float.
