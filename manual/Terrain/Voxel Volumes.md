---
order: 30
---

A **Voxel Volume** is a box of small voxels, a few centimetres across, on an Obj: grass blades a voxel wide, flowers of a handful of voxels, trees whose leaves are clouds of small cubes. It has no meshes. The GPU walks each pixel's ray through the volume's voxels, so millions of them cost about what a picture of them costs, however many there are.

Add one with **Add Component ▸ Rendering ▸ Voxel Volume**, or make one with the Hierarchy's **+ ▸ Voxel Volume**. Its least corner is at its Obj's place.

## Its size and voxels

- **Size**: how large the box is, in metres along X, Y and Z.
- **Voxels a Metre**: how small its voxels are. 16 makes voxels about 6 cm across; 8 makes them 12 cm, and 32 about 3 cm. Halving the voxels' size takes about four to five times the memory.
- **Roughness**: how rough its voxels are, from 0 to 1.

A volume is made again on a thread of its own whenever it changes; until then the last one made is drawn. A meadow 80 m by 120 m at 16 voxels a metre, with sixty trees stamped in, takes under two seconds.

## Endless ground

With **Endless** on, the volume's own ground goes on without end. It is made on the GPU round the camera, as far as **Reach** metres each way, and made again ahead as the camera moves, the nearest first. The volume's size then says only how tall it is. Its Obj's place is where the ground's pattern starts, so the same settings always make the same ground. An endless volume has no stamps yet.

A reach of 64 m at 16 voxels a metre keeps about 100 MB on the GPU. The ground round a camera standing still is made in under a second. Flying at 10 m/s, new columns 4 m a side are made on about one frame in eight, for about 0.8 ms of the GPU's time each.

## Ground from a world generator

An endless volume can take its ground from one of the project's [[Generator Graphs|world generators]]: pick it as its **Generator**. The generator's graph runs on the GPU, a voxel column at a time, and the volume shows that graph's land in small voxels, its hills, valleys and gullies to the voxel. The generator's ground at a place is the CPU's to within a millimetre or so.

- **Floor**: how high the generator's ground is at the volume's floor, in metres of the generator's world. Until it is set, it is worked out from the generator: the ground at the volume's corner stands two fifths of the way up the volume.
- **Offset**: where the volume's corner lies in the generator's world, in metres along X and Z.
- **Sea**: whether the generator's sea fills the level at its sea level. On unless turned off; see [The sea](#the-sea).

Each column's ground is what a world of blocks with no biomes of its own would lay there, from the generator's height and climate:

- grass where the ground would grow it, with the volume's **Grass** and **Flowers** on it;
- rock where the ground is steeper than 45°, or where the generator's Rock says;
- sand along the shore and in hot dry places, gravel under the sea;
- snow where it is below freezing.

Each has a pattern of its own in a few shades, as a block's face has. The volume's **Ground Height**, **Hills** and **Hill Spacing** are not used then: the hills are the generator's.

Most of the generator's nodes run on the GPU: Noise, Math, Curve, Warp, Gullies, Clamp, Blend, Terrace, Height Mask, Cells, Falloff, Landmass and Climate, which covers the Battlefield Plains, Rolling Hills, Alpine, Archipelago and Canyons starters. Some are left out there:

- **Rivers** are found on the CPU, as a world of blocks finds them, and laid into the GPU's ground (below).
- A generator's **Volume** runs on the GPU when it is made of Solid, Combine, Noise 3D, Overhangs, Tunnels and Chambers: its overhangs, tunnels and pockets are drawn in voxels, with grass and flowers on ground the overhangs moved, and rock in the walls of caves. A **Cave Network**'s great halls are left out. A Volume with Raised Land, Spires, Fins, Arches or Roof is left out, and the ground is its height made solid.
- A graph with a node that cannot run on the GPU (Plateaus, Peaks, Raised Land, Spires, Fins, Chasms, Slope Mask, a painted map) makes the volume's own ground instead, and the Console names the node.

### Rivers and lakes

A generator's **Rivers** node finds its rivers, lakes and waterfalls on the CPU, over the grid of points it works them out from. They are laid into the volume's ground as it is made:

- **The ground is cut for them.** For each 4 m column, the CPU works out how much the rivers lower the ground (or raise it, in the low rim that holds a river in) at points half a metre apart, and how high their water stands. The GPU adds that change, read smoothly between the points, to the height its graph made. Riverbeds are gravel, sand along the water's edge, and nothing grows under the water.
- **The water is drawn over them**, as a world of blocks' rivers and lakes are: still where a lake lies level, its ripples carried downstream where a river flows, a line of foam where it meets the bank, and the reflections and the view of the bed through it. Where a river drops over a cliff, its water pours down a waterfall's face in white threads, and churns at its foot.
- **Rays and changes meet the same ground.** `Voxels.Raycast` reads the same points, so it meets the riverbed the GPU draws, and a ball carved or filled lands where the ray met it.
- **Things float and swim in them.** The game's water reads the same points too: a crate or a boat with a Buoyancy floats on a river or a lake and drifts down its current, a character wading a river is pushed along by it as its **Water Push** says, and one in deeper water swims. Its current runs down the water's slope, faster the steeper, as the generator's does; a lake lies still.

The points are worked out on threads of their own as columns come into view, about 0.7 ms of one CPU core for each column, and a column is made once its points and those of the columns round it are ready. A column far from any river costs the GPU nothing more. Water stands only where the generator puts it: a pit carved below a river's level stays dry.

A script asks about the water at a place with `Voxels.WaterAt`: how high its surface stands and which way its current runs. It is found over the water, and under it down to a metre into the riverbed; not on dry ground, or deeper.

```csharp
// A fishing float rides the river downstream.
if (Voxels.WaterAt(floatAt, out var river))
{
    floatAt = floatAt with { Y = river.Height } + river.Current * Time.DeltaTime;
}
```

`Water.TrySample` reads it as well, with the sea and every other water, and `vox editor call voxels.water_at '{ "at": [12, 30, -4] }'` finds the same.

### The sea

An endless volume with a generator has the generator's sea: an [[The Ocean|Ocean]] filling the level at the generator's **Sea Level**, its waves, foam and shores as an Ocean as it comes has them. Crates and ships float on it and characters swim in it as on any Ocean. Its waves are drawn calmer over the volume's shallows; what floats there feels only the ground made for collision a few metres round it, so it rides them a little rougher than they look. There is no Ocean to add by hand.

- An **Ocean of the level's own** is the sea in place of the volume's, standing where its Obj is.
- An **Ocean on the volume's Obj** sets how the sea looks and moves, its weather, wind and colours, and it stands at the generator's sea level, not at the Obj's place.
- Turn the volume's **Sea** off for none: inland, say, where the generator's sea would lie only under the ground. A sea under the volume's floor is never drawn.

A script turns it off with `Voxels.Sea = false`, and `Voxels.SeaLevel` says how high it stands in the world. `vox editor call field.set` sets it as the Inspector does: `{"obj": "Ground", "component": "voxel_volume", "field": "sea", "value": false}`. The scene writes `sea = false`, and nothing while it is on.

Overhangs and caves cost more: every brick from the volume's floor up is asked about, though most are passed over as solid rock or open air at a look at their corners. An Alpine slope with a world's own overhangs, tunnels and chambers takes about 0.5 ms a column, four columns a frame, and about 65 MB at a reach of 64 m; the ground round a camera standing still is made in about two and a half seconds.

Running the graph about doubles what making a column costs: with Rolling Hills, about 0.08 ms of the GPU's time a column against 0.045 ms for the volume's own ground, at a reach of 64 m on an RTX 3070.

`vox editor call field.set` sets the generator as the Inspector does, by its path: `{"obj": "Ground", "component": "voxel_volume", "field": "generator", "value": "Worlds/Hills.worldgen"}`, and `generator_floor` and `generator_at` the same way. In the scene:

```toml
[obj.voxel_volume]
size = [16.0, 48.0, 16.0]
endless = true
reach = 64.0
generator = "Worlds/Hills.worldgen"
generator_floor = 40.0
generator_at = [1200.0, -300.0]
```

## Its own ground

With **Ground** on, the volume has rolling ground of its own voxels, grass on it and flowers here and there:

- **Ground Height**: how high the ground stands over the volume's floor, in metres.
- **Hills** and **Hill Spacing**: how high its hills rise over that, and how far apart they are.
- **Grass** and **Grass Height**: how much of the ground grows grass, from 0 to 1, and how tall the tallest blades are, in metres.
- **Flowers**: how many flowers grow, from 0 to 1: red with yellow middles, purple spikes, white tufts, and pink ones with yellow middles.
- **Seed**: two volumes alike but for their Seed have different ground.

## Trees

An endless volume grows trees where its ground grows grass. Each kind is one of the project's plants (a `.voxplant`, as a world of blocks grows its trees from), picked in a **Trees** row with how many grow **A Hectare**. **Add Tree** adds another kind; picking None for a kind takes it away.

- Each block of the plant is a metre of the volume's voxels. Logs are round, as thick as the plant grows them, so a trunk thins towards its top and branches are thin.
- Leaves are rounded into clumps: a thin shell of loose cubes over the crown, with nothing inside. Far off, where a volume draws its voxels two by two, leaves stay loose cubes in their own shades.
- Each kind is grown from four seeds and turned four ways, so no two trees side by side are alike. Trees stand at most one in each 5 m square, on grass: not on rock, sand, snow or steep ground, and not where the generator's overhangs or caves have moved the ground.
- The colours are the plant's wood and leaves blocks' colours.

Leaves and grass move in the level's wind, a Wind Zone's or a fair day's, as block leaves do: crowns sway, and grass leans from its foot, as hard as the wind and its gusts blow and as far as the game's foliage bends. Their shadows move with them. Trunks, branches and the ground keep still.

Each plant's voxels are made once, in each of its four turns, and shared by every tree of it: a tree stands on the grid of the volume's half-metre bricks, up to half a metre from where it would otherwise, its foot a little in the ground. A wood of 100 oaks and pines a hectare takes about 120 MB at a reach of 64 m, against 91 MB for its ground alone, and its columns cost about 0.3 ms each to make, eight a frame. The crowns' loose cubes add about 2.5 ms a frame to drawing at 1080p on an RTX 3070. Fewer trees take less.

`vox editor call field.set` sets the trees' rows as the Inspector does: `trees` is the first kind's plant, `trees#2` how many of it to a hectare, and so on, then the last to add a kind. In the scene:

```toml
[[obj.voxel_volume.trees]]
plant = "Plants/Oak.voxplant"
per_hectare = 40.0

[[obj.voxel_volume.trees]]
plant = "Plants/Pine.voxplant"
per_hectare = 20.0
```

## Box models stamped in

A [[Box Models|box model]]'s sculpted cubes can be stamped into a volume, turned into its voxels: a tree sculpted at 16 cubes a metre becomes a tree of the volume's voxels. Each stamp is the model, where its feet stand in metres from the volume's corner, how far it is turned about Y in degrees, and how many times its size it is. A model's cubes larger than the volume's voxels fill all the voxels they cover; smaller ones are rounded to the voxels.

The Inspector shows how many models are stamped. Stamps are written in the scene:

```toml
[obj.voxel_volume]
size = [32.0, 12.0, 32.0]
voxels_per_metre = 16
ground_on = true
ground = { height = 2.0, hills = 1.5, grass = 0.6, flowers = 0.2 }

[[obj.voxel_volume.stamps]]
model = "Plants/Oak.boxmodel"
at = [12.0, 2.0, 9.0]
turn = 30.0
```

## Carving and filling

An endless volume's ground can be changed: a ball carved out of it, or filled with a ground (dirt, grass, rock, sand, snow or gravel), its voxels' middles within the ball's radius. A carve takes the ground, its grass and flowers and its trees with it, and opens walls of the layers under the grass. A change goes over the ones before it, so a ball carved into rock just filled takes a bite out of it. The columns a change reaches are made again within a frame or two.

From a script, every endless volume the ball falls in is changed:

```csharp
// A crater where a shell lands, and a heap of rock where a wall is raised.
Voxels.Carve(landed, 1.5f);
Voxels.Fill(wall, 1f, VoxelMaterial.Rock);
```

`vox editor call voxels.carve` and `voxels.fill` change them as scripts do, or one Obj's volume alone, each one step to undo:

```
vox editor call voxels.carve '{ "at": [12, 24, 9], "radius": 1.5 }'
vox editor call voxels.fill '{ "at": [14, 24, 9], "radius": 1, "material": "rock" }'
```

The volume keeps its changes in order, and its scene saves them, each its middle in metres from the volume's corner:

```toml
[[obj.voxel_volume.edits]]
at = [12.0, 24.0, 9.0]
radius = 1.5

[[obj.voxel_volume.edits]]
at = [14.0, 24.0, 9.0]
radius = 1.0
fill = "rock"
```

A column takes up to 1,024 changes that reach it, and a half-metre brick 128; past those the latest are left out.

### The brush

With an endless volume selected, set its **Brush** to Carve or Fill: a ball under the pointer in the Scene view shows where it meets the volume's ground, as big as **Brush Size**. Hold the left button and drag to carve balls out along the way, or fill them with **Brush Ground**; hold Shift to do the other. Each stroke is one step to undo. The brush is the editor's own setting, kept for every volume and not saved with the scene.

### Printing models

A model made into a [[Voxel Prints|Voxel Print]] is printed into the ground as another change, kept in order with the balls: a sandbag wall or a crate of voxels, solid all through, which a ball carved later bites into. Set the **Brush** to Print to place one with a click, or call `Voxels.Print` from a script.

### Where a ray meets the ground

`Voxels.Raycast` finds where a ray first meets an endless volume's ground, as it was made and changed, and its trees' logs and leaves, within a reach; not its grass or flowers. It is worked out on the CPU the way the GPU makes the ground and places the trees, so every machine finds the same, in well under a millisecond:

```csharp
// Dig where the player looks.
if (Voxels.Raycast(camera.Position, camera.Forward, out var hit, 6f) && Input.Pressed(dig))
{
    Voxels.Carve(hit.Point, 0.5f);
}
```

`hit` has the point, on the face of the voxel met, which way that face looks, how far along the ray, and the volume's Obj. `vox editor call voxels.raycast '{ "from": [0, 40, 0], "direction": [0, -1, 0] }'` finds the same.

## Collision

Characters and bodies collide with an endless volume as with any ground: they stand on its voxels, bump into its trees' trunks and branches, and fall into what is carved out under them. Leaves, grass and flowers are walked through. Nothing needs adding: an endless volume collides as soon as it is in a scene that plays.

The ground is never made whole for physics. Its solid voxels are made into shapes in chunks of 32 voxels a side (2 m at 16 voxels a metre), only round what moves: each character, and each Obj with a Rigidbody, where it is and where it will be in half a second. A chunk is made in the background as something comes within about 5 m of it, the nearest first, or at once for one within a metre that isn't ready. Chunks are let go once everything has gone well away. A change made to the volume makes again only the chunks it reaches, and wakes the bodies resting there.

A character walking at 6 m/s over a generator's hills, through a wood of 100 trees a hectare, costs about 0.2 ms a physics step; a step that has to make a chunk at once takes about 5 ms, every two metres or so going down steep slopes. Over the volume's own hills a step stays under 0.5 ms.

Physics rays (`Physics.Raycast`) meet only the chunks made round what moves; `Voxels.Raycast` meets the whole ground.

## How it is drawn

A volume is lit as everything else is: by the sun, with the shadows of meshes, blocks and terrain on it, by lamps, by the sky's light and bounced light, with ambient occlusion and reflections, and in the scene's fog. Its voxels shadow each other, traced through the volume towards the sun.

By the sea, a volume's ground is part of the sea floor the waves feel, as meshes standing still are: the waves calm and shorten over its shallows, and foam runs along the shore where they meet it. The floor is found by walking straight down through the voxels round the camera, as far out as the volume reaches.

Far off, where a voxel is smaller than half a pixel, it is drawn as groups of two by two by two, and further still as whole bricks of eight voxels a side in their average colour, so far grass and leaves don't shimmer.

The Profiler's **Voxel coarse rays**, **Depth (voxels)** and **Opaque (voxels)** say what volumes cost. A meadow 80 m by 120 m with sixty trees, seen from in it at 1920 × 1080 on an RTX 3070, takes about 0.1 ms for the coarse rays, 2 ms for the depth pass and 3.8 ms to light.

## What it doesn't do yet

- Its voxels cast no shadow onto meshes, blocks or terrain.
- It isn't turned or scaled with its Obj.
- It is drawn whole, however far away it is; an endless one ends at its reach.
- An endless one made by a generator has no Cave Network halls yet. Its rivers' water doesn't flow into a pit carved beside it. Its sea is under the ground inland too, so a cave dug below the sea's level is under water; turn the volume's Sea off where there is no coast.
- An endless one's trees can't be felled, and every tree of a kind is one of four plants. Grass blades short enough to stand wholly in the half-metre brick with their soil keep still in the wind.
- A volume of a set size can't be changed yet, only an endless one, and only by balls and prints: not a voxel at a time, or over the network. A volume of a set size doesn't collide yet; an endless one does.
