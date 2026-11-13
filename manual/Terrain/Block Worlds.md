---
order: 17
---

A **Block World** is ground made of blocks: a cube each, of any of the project's [[Blocks|blocks]], grass, dirt, stone or anything you make, from big blocks to build with down to small ones to sculpt with. It reaches from its Obj's position along +X, +Y and +Z, and only the Obj's position places it.

## Making a world of blocks

Select an Obj and pick **Add Component ▸ Terrain ▸ Block World**. It starts as flat ground 64 blocks a side and 8 up, grass over three layers of dirt over stone, on a new asset of its own in the project's **Blocks** folder, named for its Obj: `Blocks/Ground.blockworld`. Its grass, dirt and stone are the project's blocks of those names, `Blocks/Grass.block` and the rest, made the first time a world wants them, and its brush starts with the grass. Every Block World that picks the same asset has the same blocks.

Along the top of the Block World in the Inspector are its tools: **Build**, the brush and the block it builds with, and **Settings**:

| Setting | What it does |
| --- | --- |
| Data | The `.blockworld` asset its blocks are |
| Block size | How many metres a block is a side, from 0.1 to 10: smaller blocks make the same world finer, not smaller (see below) |
| Smooth | Whether its surface runs smoothly between its blocks rather than as cubes: a [[Voxel Terrain]] |
| Voxels | How many small cubes each cube's faces are drawn as made of, along a block's side: 0 for plain faces, 8 or 16 for a world built of small cubes (see [Faces of small cubes](#faces-of-small-cubes)) |
| Relief | With Voxels, how many small cubes deep the deepest of them are sunk into a face, from 0 to 4 in halves |
| Ground | **Flat**, or **Generated** by a world generator (see below) |
| Ground size | How many blocks its ground reaches along X and Z |
| Ground height | With flat ground, how many blocks high it is |
| Top block, Under block, Base block | The blocks its ground is made of, each picked from the project's blocks: its top, the layers under that, and the rest. None for air |
| Under depth | How many layers of the under block there are |
| Cast shadows | Whether its blocks cast the sun's shadows |

A change to its ground makes its blocks again at once, what you built on it kept. Each change is one step to undo, and is written to the asset when you save the level.

Changing **Block size** keeps the world as it is in metres and makes it again from blocks of the new size. Its ground stays as wide, as high and as deep, so the rows counted in blocks (Ground size, Ground height, Under depth, Lowest block, a block's cover Depth) are counted again: halve the block size and each doubles. What you built stays where it was: a wall a metre thick is two blocks thick at half the size, and in a Voxel Terrain the ground's surface stays where it lay. Going to larger blocks loses what was finer than they are. A block is 0.1 m a side at the smallest and 10 m at the largest, and flat ground is at most 4096 blocks a side, which can keep its blocks larger still. Dragging a number by its label, as Block size or Ground size, shows the number as you drag and makes the world again once you let go. A large world, such as one of small blocks, is made again in the background: the Inspector shows the new numbers at once, and the Scene view shows the world as it was until the new one is ready, after a second or two. Undo takes it back in one step; undoing before it is ready lets the change go.

Its blocks are made from its ground as it loads, the same on every machine. What you built since is kept beside the `.blockworld` in a `.blocks` file of the same name, holding only the chunks you changed, each as its changed blocks one by one while they are few, or whole once that is smaller: a house on a field takes a few hundred bytes. Keep the two together when you move or copy them. A built game ships both.

## Generated ground

With **Ground** set to **Generated**, a [[Generating Terrain|world generator]] makes the ground: the same mountains, coasts and climate a terrain's Generate layer lays, a column of blocks at a time. Each column rises as high as the generator's ground there, its top the top block, the under block below that, and the base block the rest.

| Setting | What it does |
| --- | --- |
| Generator | The `.worldgen` its ground comes from |
| Offset | Where its corner is in the generator's world, in metres |
| Lowest block | How far down its ground goes, in blocks from its corner: below that is air |
| Overhangs | How far its steep ground is pushed in and out, in metres, so cliffs lean, bulge and overhang |
| Caves | How much of the ground below is hollowed out, from 0 for none to 1: long tunnels, and chambers here and there; and over 0, a [[Caves|Cave Network]] of great halls joined by wide tunnels further down, with ways up and mouths in the hillsides |
| Rivers | Whether rivers, lakes and waterfalls run down the ground, where its generator makes no water of its own, as a generator written before generators had rivers does. On for ground newly generated; a [[Generator Nodes#Rivers|Rivers]] node over the generator's ground |
| Sea block | The block the air below the generator's sea level is filled with, one that is [[Block Worlds#Water|water]]; None for none |
| Biomes | The [[Biomes|biomes]] its ground has: each place takes its top, under and deep blocks from the biome it is |

Overhangs push cliffs and leave hillsides alone: ground steeper than about 30° is pushed a little, and ground steeper than about 45° by the whole amount. The bulges are about 14 m across, so the ground hangs over open air once Overhangs is 9 m or so; under that, cliffs lean and bulge. The far rings of a world without end, whose blocks are too large to hold a bulge, are left unpushed.

The ground's layers follow the pushed ground. The top block is whatever has open air over it, with the under block below, counted down each column, so the under block never shows through where the ground was pushed out. Under an overhang the count starts again on the ground below, and the overhang's underside has no top block.

Caves keep three blocks under the surface whole, so they open onto it only where a slope cuts into them. The sea fills only the open air over the ground: caves under it stay dry.

Where a cave or a passage cuts under ground that leans out over it, the rock left hanging in the air is cleared: no piece of ground of 256 blocks or fewer stands loose, joined to nothing. Larger ground standing apart, an island in the sky or a slab of rock, is there on purpose and stays. A piece reaching into the chunk column beside is followed on there, so each column clears it alike.

### Tall worlds

Nothing limits how high a generator's ground stands or how far down **Lowest block** goes: mountains and tablelands hundreds of blocks high, with rock hundreds of blocks deep under them for caves in many layers. Set Lowest block as far under the world's corner as the deepest cave you want, and a little more.

Depth costs little. Rock that nothing is hollowed out of is kept as whole chunks of one block, which are not made a block at a time, take almost no memory and are never drawn. A Cave Network, Arches and Raised Land say how high and how low they reach over each column, so only the cells between are asked about. A chunk column of plain ground takes about 3 ms at 256 blocks tall and 4 ms at 1024; one over three layers of caves about 28 ms and 39 ms. Tunnels, Chambers and Noise 3D can hollow rock at any depth, so with those every block down to Lowest block is asked about: keep such a world shallower.

### Deep ground

Deep does not have to mean underground. A generator can cut chasms hundreds of metres down and leave their floors open to the sky, or roof them over at the land's height with holes where light falls through (see the [[Generator Nodes#Chasms|Chasms]] and [[Generator Nodes#Roof|Roof]] nodes, and the **Rifts** preset). A chasm's floor is the generator's ground: it takes the top block, the under block and the base block like any other column, and plants and structures stand on it. A roof's top takes the blocks of ground as high as it stands, so the land over a cavern looks like the land round it.

A biome can keep to ground that is sunk, by its **Sunk** range: how far under the land round about its ground lies, in metres, as the generator's Output's **Sunk** says. A biome with a Sunk of 40 m to 2000 m covers every chasm and cavern floor at least 40 m down, whatever the climate there, so a forest of ferns can grow at the bottom of a rift in desert country. Where no biome keeps to sunk ground, a chasm's floor is the biome of its climate and height, as anywhere else.

### Biomes

A world's ground is one kind of country until it has [[Biomes|biomes]]. List some under **Biomes**, or press **Add Starter Biomes** there for eight ready ones, and each place takes the blocks of the biome its climate and height make it: snow on the heights, sand in the dry heat, moss in the wet. The world's own Top, Under and Base blocks are used wherever a biome names none.

### Covers

Biomes replace covers, and a world that lists biomes shows none. A world without biomes can still have them.

A block can **cover** generated ground in place of its top block, by the ground's climate and height. Under **Build**, pick the block for the brush and tick **Cover**, and its rows show:

| Setting | What it does |
| --- | --- |
| Biome | Sets its temperatures and humidities to a biome's at once (see [[Surfaces#Biomes|Biomes]]) |
| Temperature, Temperatures | Covers only between two temperatures, in °C |
| Humidity, Humidities | Covers only between two humidities, from 0 for dry to 1 for wet |
| Height, Heights | Covers only between two heights in the generator's world, in metres |
| Depth | How many blocks deep it lies |

Snow covering where it is below freezing, sand where it is hot and dry and low, gravel on the tops. Where two blocks would cover the same ground, the one the world came to hold later lies on top. A cover is the world's, not the block's: the same snow can cover one world's peaks and none of another's. A change to the generator makes the ground again, with what you built on it kept.

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
| Far distance | How far coarser blocks are drawn beyond, in metres: up to 16 km with metre blocks (see [[Block Worlds#The far view]]) |

Beyond its view distance, it is drawn in rings of coarser blocks, each ring's blocks twice as large as the one inside it, so mountains stand on the horizon at a fraction of the cost. A coarse ring shows the ground until the finer one inside it is drawn, and never where it is. Only its own blocks near the camera can be built on and stood on.

### The far view

From the third ring out, the world is the **far view**: up to five more rings, the last with blocks 128 times as large as the world's own, so with metre blocks and a View distance of 128 m the ground reaches 16 km. The far view is made from the generator's heights, climate and sea alone, as columns of blocks one over another, which is quick to make and small to keep. It shows the ground's surface, its layers and its sea, but not overhangs or caves: they are too small to see from so far off.

Each piece of the far view is kept in the project's `.voxear/FarView` folder once made, so ground seen once comes back at once, and is made again when the world or its generator changes. What you build shows in every ring once the camera has gone from it, wherever it fills at least an eighth of a ring's block: a castle stays on the horizon, and a fence post does not grow into a tower. Ground taken out shows only when the camera is near it again.

A minimap and a map of the world are made from the same ground as the far view, with what was built and dug: see [[Maps and Minimaps]].

Its own blocks are made a column of chunks further out than they are drawn, and each column's faces are made on those threads too, once the columns round it have their blocks. So every column is made once, its edges meeting the ground beside it, and the game never waits for one.

A [[Block Worlds#Water|sea]] runs on across the rings unbroken: it stands as high in every ring, and its water shows no sides where they meet, nor at the edge of what is made, where the world goes on.

What you build is kept as it goes out of reach and made again as it comes back, and saving keeps it all, near or away, in its `.blocks` file. In a game it is made round the camera; in the editor, round where the Scene view looks from.

With the [[Floating Origin]] switched on, positions stay exact however far the camera goes: the world's middle moves along with it, and the chunks already made stay as they were.

### On slow machines

How much making worlds without end may spend is the project's, in **Project Settings ▸ World ▸ Block Worlds**, and a game's options can change it while the game runs. **Quality** picks a preset; changing a number under it makes it **Custom**.

| Setting | What it does |
| --- | --- |
| Quality | Low, Medium, High or Ultra; Custom once a number is changed |
| Threads | How many threads make terrain at once, heightmap tiles too |
| Columns at once | How many columns of chunks are made at once, the nearest first |
| Upload a frame | How many megabytes of chunks go to the GPU a frame: a world with more to show comes over several frames, shown as it was until then |
| View distance | How far a world's own blocks reach, in metres, at most |
| Far distance | How far its coarser blocks reach, in metres, at most |

| | Low | Medium | High | Ultra |
| --- | --- | --- | --- | --- |
| Threads | 2 | 4 | 8 | 16 |
| Columns at once | 2 | 8 | 24 | 48 |
| Upload a frame | 4 MB | 8 MB | 16 MB | 32 MB |
| View distance | 64 m | 128 m | 192 m | 256 m |
| Far distance | 1 km | 4 km | 8 km | 16 km |

A project starts on High. A world is drawn as far as its own View distance and Far distance say, and no further than these: on High, a world of 128 m and 1 km is drawn just as it says, and on Low only to 64 m and 1 km. A machine never uses more threads than one fewer than its cores, whatever the setting says.

A change takes hold at once, even while a world is being made round the camera, and nothing already made is made again. From a script:

```csharp
World.Quality = WorldQuality.Low;
World.ViewDistance = 96f;
```

`World.Threads`, `World.ColumnsAtOnce`, `World.UploadMegabytes`, `World.ViewDistance` and `World.FarDistance` read and set the numbers, and setting one makes `World.Quality` Custom. What a script sets is not written to the project: keep the player's choice with the game's own settings, and set it again as the game starts.

## The blocks a world holds

A world can hold any [[Blocks|block]] of the project, with nothing to set up. Pick one for the brush, for the ground or for the sea, and build.

The first time a world holds a block, it gives the block a number, and it keeps that numbering in its `.blockworld` file. You never see it and never edit it. A number is given once and never changes, so nothing you built changes as the project gains blocks or loses them.

A world draws its faces 256 ways at most: a way for each colour or material a face is drawn with, so a plain block takes one and a block with a top and a bottom of its own takes three. A world with no room left says so once in the Console, and the block is not placed.

A block deleted from the project while a world holds it is drawn bright pink in that world, so it is easy to find. The [[Blocks#The Blocks panel|Blocks panel]] says which worlds hold a block before it deletes one.

### Worlds made before blocks were assets

An older world kept its kinds of block in its own file. When such a world opens in the editor, each kind you made by hand becomes a block, named after the kind, in a **Blocks** folder beside the world, and the Console says how many. The world then holds those blocks by the numbers its kinds had, and its file is written at once. Nothing built changes, and it is drawn as it was.

A game loads an older world as it is, without the editor, and draws it the same.

## Water

A block whose **Liquid** is **Water** is water, drawn see-through with the same look as the sea and Pools. Its **Color** is the water's own colour, the colour it shows where it is deep: lake green, sea blue or a muddy brown. Water has no materials, so its Material, Top and Bottom rows go. Build with it as with any block.

Water shows its faces only where it meets air: a pond in a hollow is its top alone. The ground under it and round it shows through it, bent as light bends going into water, and dimmed by as much water as the view goes through: a shallow edge is clear, a deep middle shows its colour. Its top ripples, catching the sky and the sun, with a line of foam where it meets the shore. From under the water, its top shows the world above it.

Nothing stands on water. Physics leaves it out, so things fall through it to the ground under it, and a Rigidbody with **Buoyancy** floats in it as it does on a Pool: the top of the water in its column is the surface.

### Flowing water

Water flows. Each block of water you place is a **source**: it stays full and never runs dry. Water runs out of it, down first and then out over the ground or over still water, an eighth of a block lower for each block it goes, for seven blocks at most. Over an edge it falls as a column to the ground below and runs on from there. Falling water runs out only once it lands on ground, so a waterfall keeps to its column, and water falling into a lake joins it. A block with a source on each of two sides becomes a source itself, so a gap between two pools fills to the brim. Take a source away and the water it fed dries up behind it. Dig into the bank of a pond or the sea and the water runs into the hole.

Water flows five blocks a second, the same on every machine, both in play and in the editor as you build; while a game is paused, it stands still. It starts wherever something changes, so a generated sea stays as it was made until you dig into it or build beside it. It stays within the chunks its world reaches: at the world's edge it stops as if at a wall. Saving keeps the water where it has run to.

For a sea, pick a block of water as the **Sea block** of generated ground: the air below the generator's sea level is filled with it.

### Rivers and currents

A generator with a [[Generator Nodes#Rivers|Rivers]] node, as every land preset has, fills its rivers and lakes with the Sea block too, each column as high as its water stands. A world whose generator has none, as one written before presets had rivers, gets them with its **Rivers** setting on. Where a river drops over a cliff, its waterfall is a curtain of falling water over a pool at its foot.

A river's blocks are sources, so it never runs dry, and each keeps which way its river flows and how high its water stands in it, so its surface comes down in eighths of a block as water placed and running out does, not a block at a time. Its ripples drift the way it flows. Water flows of itself too: water running out sideways flows away from what feeds it, and falling water falls.

Flowing water pushes:

| What | How it goes |
| --- | --- |
| A Rigidbody with **Buoyancy** | Floats and drifts with the current |
| A Rigidbody without | Is dragged along the bed as hard as it lags the water: a crate rolls downstream, a boulder hardly stirs |
| A Character Controller | Is carried along as much as it is in the water, fully once in up to its middle, as its **Water push** says. Under a waterfall it is pushed down |

Scripts read the flow at a point with `Water.FlowAt`.

In a [[Voxel Terrain]], water is smooth too: its surface runs between its blocks as the ground's does. A full block's surface is its top, and flowing water stands as high as it flows. A generated sea lies flat at its generator's sea level; it lies over the ground wherever the ground is under it, however shallow, and its surface runs on a block into the shore, so the waterline is where the ground rises through it, with no gaps. It flows as it does among cubes.

With the camera in the water, the view is the same as under a Pool: the water's own colour thickening with distance, light fading with depth, the surface from below, a waterline across the lens as you come up. The water round the camera is found from the top of its column, as far as that top runs along X and Z, so the ground beside a pond stays dry to look at. The waterline lies level, as the water's top is flat, and the ground under the water is lit as it is from far off, whether the camera is near the water or not.

Not done yet: the ground under the water has no dappled light, as a Pool's bed has.

### Rough seas

The sea is flat in calm weather. Under a [[Storms|storm]], and under windy weather over the whole level, it rises and falls in a swell: long waves that run with the wind, higher the harder the storm blows, calm again beyond its edge. A crest stands at the sea's own height and a trough as far under it as **Swell Height**, three metres in the hardest storm unless you change it. Crests break into white water.

Only the sea is rough: water that stands at the generator's sea level. Ponds, rivers and anything you build with water blocks stay calm. In a world with no generated ground, all still water is its sea.

No block changes. The swell is in how the water is drawn and in what [[Floating Objects|floats]] on it, so nothing of it is saved or sent, and it is the same on every machine: it is worked out from the storms and their clock. A Rigidbody with **Buoyancy** rides it, `Water.HeightAt` gives its height and `Water.FlowAt` its water's movement, up and down and along with the waves.

The level's **Storms** component (see [[Storms#Storms that come by themselves|Storms]]) says how the sea is drawn:

| Setting | What it does |
| --- | --- |
| Sea Look | **Small Cubes**, the usual: quarter-block cubes that rise and fall. **Block Steps**: columns a block wide that stand half a block higher or lower. **Smooth**: a rolling surface. **Flat**: no swell, whatever the weather |
| Swell Height | How far the deepest trough of the roughest sea lies under a crest, in metres |
| Storm Whirlpools | Whether strong storms raise whirlpools over deep water |
| Maelstroms | Whether great whirlpools stand at places of their own in a generated sea |
| Night Glow | How much warm seas glow blue at night, where the water breaks and in specks that come and go. 0 for none |

From scripts these are `Sea.Look`, `Sea.SwellHeight`, `Sea.StormWhirlpools`, `Sea.Maelstroms` and `Sea.Glow`.

### Whirlpools

A whirlpool draws the sea down into a funnel of steps with white arms that turn. Its water runs round its middle and in, fastest a little way out from it, and down at the middle itself, so what floats is carried round, drawn in and pulled under.

| Whirlpool | Where |
| --- | --- |
| A storm's | Under a storm blowing at more than seven tenths of its strength, where the sea is ten metres deep or more. It drifts with its storm and goes as the storm weakens |
| A maelstrom | At seeded places in a generated sea, where the water is fourteen metres deep or more all round: 22 to 38 m across, drawn 7 to 12 m down. The same for every player |
| Your own | `Sea.AddWhirlpool(position, radius, depth)`, which gives a `Whirlpool` to `Remove()` later |

```csharp
namespace Game;

public sealed class Swallower : Behaviour
{
	protected override void OnEnable()
	{
		Sea.Swallowed += Sink;
	}

	protected override void OnDisable()
	{
		Sea.Swallowed -= Sink;
	}

	protected override void Update()
	{
		// A swimmer in the middle of one goes under.
		if (Sea.WhirlpoolAt(Transform.Position) > 0.9f)
		{
			Log.Info("Dragged under");
		}
	}

	private void Sink(Obj boat)
	{
		boat.Destroy();
	}
}
```

`Sea.Swallowed` is raised once for each floating Rigidbody as it reaches a whirlpool's middle. `Sea.WhirlpoolAt` is how far into one a point is, from 0 at its rim to 1 at its middle, for what moves without a Rigidbody.

Not done yet: from under the water its top looks flat in any weather; the swell does not calm in the shallows or break on a shore; and the sea does not freeze where the climate is cold.

## Building

With **Build** showing, the brush works in the Scene view over the selected world's blocks, its outline round the blocks it would change.

| Setting | What it does |
| --- | --- |
| Brush | **Place** puts the picked block against the face under the pointer; **Break** takes the block there away; **Paint** turns it into the picked block; **Model** builds a model of blocks (see below); **Use** uses the block clicked, as a player would; **Link** links a lever or a button to the blocks it drives |
| Size | How many blocks across it reaches: 1 for a block at a time, more for a ball of them |
| Block | The block it builds with, picked from all the project's blocks. The ones you built with last come first |
| Model | With the Model brush, the `.vox` model it builds |

A block double-clicked in the [[Blocks#The Blocks panel|Blocks panel]], or dragged from it onto **Block**, is picked too.

A [[Blocks#Blocks of parts|block of parts]] is put down standing as a player would put it: a stair rising away from the view, a slab lying on the face clicked, a door on the far side of its block with its hinge on the side it was put nearer. Its outline shows its boxes as they will stand. Press **R** to turn it on a quarter, or, for a slab, a lever or a button, on to its next side. A door put down takes its upper block over it, and breaking either block of it takes both.

With **Use**, a click uses a block as a player would: a door opens, a lever turns over and drives what it is linked to. With **Link**, click a lever or a button, then each block it should drive: a line shows each link. Click a block linked already to take its link away.

Click to build once; drag to go on, the brush working again as the pointer moves on by half its size. A brush that builds reaches through water to the ground under it, and what it places takes the water's place; with water itself picked, or breaking, it meets the water. Hold **Shift** to break while placing, and to place while breaking. Each stroke is one step to undo, the number a world gave a new block with it. The brushes work while playing too, on the world selected: what you build there goes when play stops, as everything played does.

## Models of small blocks

A **Block Model** is a `.vox` file, as voxel sculpting tools save them: a model of coloured blocks, or a scene of several placed together. Put one in the project like any other asset. With the brush on **Model**, its **Model** row picks one, its outline shows where it would stand, and a click builds it standing on the face under the pointer, its middle over the block clicked. Up in the file is up in the world, and each model of a scene stands where the scene puts it.

Each of its colours builds with a colour of the world's own: one the world has already, or a new one. A model's colours are kept in the world's file with its numbering, out of sight, and are no blocks of the project, so a model of two hundred colours is not two hundred files. Once the world has no room for more ways to draw a face, a colour builds with the nearest one there is. A click builds the model once, and is one step to undo, the colours it added with it.

For a world of small coloured blocks to sculpt with, make **Block size** small before you build: at 0.1 m, a model 32 blocks high stands about three metres tall.

## Plants a world grows

A Block World can grow its own grass, flowers, bushes, rocks and trees, and what lies about among them: sticks, fallen logs, stumps and pebbles. Under **Settings**, the **Add plant** row takes a model: pick one there, or drag one onto the row from the Project. It becomes a [[Foliage]] under the world's Obj that grows by itself on the world's top block, a plant here and there. Select that Foliage to change where and how thickly it grows (see [[Foliage#Growing on a Block World]]).

The row also offers the **starter plants**, each made of small cubes to sit well among blocks, eight of them for the sea's bed. Picking one writes its model into the project's `Plants` folder as a [[Box Models|box model]] and grows it as suits it. **All Starter Plants**, at the top of the list, adds every one the world does not grow yet, in one step:

| Plant | Where it grows |
| --- | --- |
| Grass Tuft | On more than half the world's top blocks |
| Tall Grass | In patches about 9 m across, where it is not dry |
| Flower, Daisy, Bluebell | In patches: red flowers anywhere, daisies where it is mild, bluebells where it is cool and wet |
| Fern | In patches where it is wet |
| Mushroom | Near trees, where it is cool and wet |
| Bush | Here and there, about 7 m apart |
| Dry Bush | Where it is hot and dry, on any ground |
| Tree | Where it is wet enough, on gentle ground, drawn out to 400 m, and beyond as a wood of blocks. Its trunk collides |
| Birch | Where it is mild, drawn and shown far off as the Tree is. Its trunk collides |
| Pine | Where it is cool, drawn out to 400 m, and beyond as a wood of blocks. Its trunk collides |
| Dead Tree | Rarely, anywhere. Its trunk collides |
| Stump, Log | Near trees. Both are solid: a stump is stepped onto and a log climbed over |
| Stick | Near trees, lying any way round |
| Pebbles | Near rocks and boulders, and as **Shore Pebbles** in patches along the sea |
| Rock | On any ground, steep slopes too, in many sizes. It is solid: walked round and climbed on |
| Boulder | Rarely, on any ground, three metres across and more. It is solid |
| Seagrass | Under water one to twelve metres deep, in patches about 18 m across, wherever the sea is above freezing |
| Kelp | In cool seas, 18 °C and under, in water nine metres deep or more: stalks eight metres tall in forests about 40 m across |
| Branch Coral, Brain Coral, Table Coral, Fan Coral | In warm seas, 20 °C and over, two to fourteen metres down, all four in the same reefs about 30 m across |
| Tube Sponge | Three to thirty metres down, where the sea is 8 °C or warmer |
| Clam | Two to twenty metres down, here and there |

The grasses, flowers, ferns, bushes and trees bend in the [[Wind]] and away from whoever walks through them; stones, stumps, logs and sticks do neither. Seagrass, kelp and the fan coral sway with the water, more under a storm; the other corals, the sponge and the clam stand still.

A starter plant that keeps to the world's top block also grows on the tops of the [[Biomes#Starter biomes|starter biomes]] that things grow on, where the project has them: Grass, Moss, Needles Floor, Jungle Floor and Dry Grass. So add the starter biomes first. Sand, snow and rock stay bare but for stones, dry bushes and dead trees.

What grows near something keeps to it by its Foliage's **Near plant** and **Within** (see [[Foliage#Near other plants and blocks]]): the sticks, logs, stumps and mushrooms near the Tree, the Pine and the Birch, and near the leaves of the starter trees of blocks; the pebbles near the Rock and the Boulder. Take the trees away and those have nothing to grow near.

A starter plant is a model like any other. Open it in the Model window to reshape or recolour it, and every one of them in the world changes with it. To grow a plant of your own, sculpt a box model from cubes (see [[Box Models#Plants and trees]]) and pick it on **Add plant**.

A world without end grows its plants as its ground is made round the camera, the same plants every time. Trees reach past the world's own blocks, onto the coarser ground beyond, as far as they are drawn, and from there the far view shows them as woods of their **Far block**: for a starter tree, a block of its leaves' colour made beside its model. A map draws a plant in that block too, each where it grows (see [[Maps and Minimaps]]). A plant goes when the block under it is dug or built over.

Plants grow under leaves: a tree of blocks has grass and flowers on the ground beneath its crown, and none on top of it.

### Trees of blocks

**Starter Trees of Blocks**, on the same row, gives a generated world trees built of its own blocks: an oak, a birch and a pine, each grown afresh at every place so no two are alike, with [[Blocks#Bushy leaves|bushy leaves]] that sway and, on some oaks and birches, [[Blocks#Vines|vines]]. They are [[Structures#Trees of blocks|structures]] the world's generator builds, so they are dug, stood on and built on as the ground is. Their logs and leaves are blocks made in the Blocks folder beside the world, each drawn with a small picture made with it, in a `Pictures` folder there: bark for a log, and cut-out leaves. Paint over a picture, or give a block a material of your own, and every tree of that kind changes with it.

Trees of blocks stand only on the world's own blocks, out to its **View distance**, and the far view does not show them. The starter Tree, Birch and Pine are models, drawn far and shown as woods beyond: use either, or both.

## How blocks are drawn

The blocks are kept in chunks of 32 a side, only the chunks with blocks in them, so a world is as large as its blocks and no larger. A face is drawn only where a block meets air or water, and faces side by side drawn the same way are drawn as one, so a field of grass is a handful of quads. Each corner of a face is a little darker as the blocks round it close it in, which gives edges and hollows their depth.

The sheets of [[Blocks#Bushy leaves|bushy leaves]] and of [[Blocks#Vines|vines]] are kept beside a chunk's faces, two words each as a face is, and drawn from either side while the chunk is within 160 blocks of the camera.

The camera draws only the chunks in view, and of each only the faces turned towards it. Caves and rooms closed off from it are passed over too: each chunk notes which of its sides see each other through its air and water, and only the chunks the camera can see into through them are drawn, so a mountain full of caves costs no more than its outside until a tunnel opens onto one. The sun's shadows fall from every face. Blocks light the world round them too, as the GI's rays see them.

Water's faces are drawn after the rest of the scene, with the sea and Pools: every face of the water in the chunks the camera sees, from either side, flowing water as high as it stands. Water casts no shadow, and the GI's rays pass through it.

## Faces of small cubes

Set **Voxels** and every face of the world's cubes is drawn as if built of small cubes: that many along a block's side, in one grid over the whole world, so faces side by side and round a corner line up.

- Each small cube is one colour: its part of the block's picture. Give a block a picture as many pixels across as Voxels, 16 for 16, and each pixel becomes a small cube.
- A small cube is sunk into the face where its part of the picture is darker than the picture is all over, so the mortar of a wall lies behind its bricks and the gaps of planks behind the planks. **Relief** is how many small cubes deep the darkest go: 0 leaves the face flat, a mosaic of squares.
- A face with no picture, only the block's colour, has small cubes sunk here and there, more together in some places than others, each a slightly different shade.

The small cubes are worked out as each face is drawn. The world's blocks, its mesh, its collision and what scripts read of it are the same with Voxels as without, and it costs nothing to change. A block's outline stays a cube: the small cubes are cut into its faces, never raised from them, and cast no shadows of their own. Far enough off that a small cube is under a pixel across, the faces are drawn plain, as the same picture looks from there.

In the file, they are `voxels` and `relief`:

```toml
block_size = 1.0
voxels = 16
relief = 2.0
```

A Voxel Terrain, drawn smooth, has no faces to cut and no Voxels row.

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
| `Blocks.All` | Every block of the project |
| `Blocks.DefinitionAt(position)` | The block at a point; `Block.None` for air |
| `Blocks.Raycast(origin, direction, out hit, maxDistance)` | The first block a ray meets: its Obj, point, face, the block's middle, and `hit.Definition`, the block it is |
| `Blocks.Set(position, block)` | Sets one block |
| `Blocks.Fill(from, to, block)` | Sets every block in a box |
| `Blocks.Ball(middle, radius, block)` | Sets every block in a ball |
| `Blocks.Break(middle, radius)` | Takes away every block in a ball |
| `Blocks.Place(hit, block)` and `Blocks.Break(hit)` | Puts a block against the face a ray met, or takes away the block it met |

A ray passes through water to what lies behind it, as a hand reaching into a pond does, and a block placed where water is takes the water's place. Pass `liquids: true` to **Blocks.Raycast** to meet the water instead, as a bucket does.

Any block of the project can be placed in any world: one that has not held it numbers it then. A game writes no world's file, so that number lasts as long as the game runs.

Breaking by a block's rules, how long it takes with a tool, what it drops, and items that place blocks are in [[Blocks and Items from Scripts]].

### Doors, levers and how blocks stand

A script uses a block as a player does, and reads and sets how a [[Blocks#Blocks of parts|block of parts]] stands:

```csharp
public Block Door;
public Block Stairs;

// Open the door the player looks at; a listener may keep it locked.
if (Blocks.Raycast(eye.Position, eye.Forward, out var hit, 4f))
{
    Blocks.Use(hit, Obj);
}

// A stair facing east, upside down, and a door turned as the player looks.
Blocks.Set(at, Stairs, new BlockState(BlockFacing.East, BlockHalf.Upper));
Blocks.Place(hit, Door, eye.Forward);

// A lever opening a gate across the yard.
Blocks.Link(lever, gate);
```

| Call | What it does |
| --- | --- |
| `Blocks.Use(position, user)` and `Blocks.Use(hit, user)` | Uses a block: opens a door with its pair, turns a lever, presses a button. False where it is no block that is used |
| `Blocks.Using`, `Blocks.Used` | Before a block is used, with `Cancel` to leave it as it is; after, with what it did and whether it is open now |
| `Blocks.StateAt(position)` | How the block there stands: its `Facing`, `Half`, `Open`, `On` and `Hinge` |
| `Blocks.SetState(position, state, animate)` | The block there stood another way. Only opening or shutting moves by its motion, with its pair and what a lever drives |
| `Blocks.Set(position, block, state)` | Sets a block standing a way |
| `Blocks.Place(hit, block, looking)` | Puts a block against the face a ray met, standing as someone looking that way puts it down, a door with its upper block |
| `BlockState.Placed(shape, hit, looking)` | How a block of a shape would stand put down so |
| `Blocks.Link(from, to)`, `Blocks.Unlink(from, to)`, `Blocks.LinksFrom(from)` | A lever or button's links to the blocks it drives |
| `block.Shape`, `block.IsUsable` | A block's shape, and whether a player can use it |

A block's [[Blocks and Items from Scripts#Rules of your own|rules of its own]] answer `CanUse`, to keep a door locked until its key is carried, and hear `OnUsed`.

Using a block gives the same changes on every machine that has the same blocks: in a game over the network, send where a player used a block, and use it there on each machine, or send the changes from `Blocks.CaptureEdits` to a player who joins. How each block stands, and the links, are in those changes.

### Blocks by number

Every call also takes a block's number in its world, from 1, with 0 for air: `Blocks.At(position)`, `hit.Block`, `Blocks.Set(position, 3)`. A number is one world's own. For a block the world first held while the game ran, it is this machine's own too, so over the network send the `Block`, or the item that places it, never its number.

Physics and drawing follow a change at once: only the chunks it changed are made again, and those beside them whose faces it meets. A save of the scene keeps what scripts changed, as only what differs from the world as its level built it, and `Blocks.CaptureEdits` and `Blocks.ApplyEdits` give the same bytes to a player joining a game. The bytes say which block each number given in play is, so they read in a game that numbered its blocks another way.

## Collision

A world of blocks collides as it is drawn: physics stands things on its blocks, a mesh for each chunk, and rays from **Physics.Raycast** meet them, its Obj as what they hit. Its blocks of parts collide as their boxes stand, fences and walls a block and a half high, and a character on it steps up half a block by itself (see [[Blocks#Blocks of parts|Blocks of parts]]). It needs no Collider. Its Obj's **Layer** is its blocks' layer. Its water does not collide: things fall into it, and those with Buoyancy float.
