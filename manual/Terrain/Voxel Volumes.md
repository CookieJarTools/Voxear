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

## Seen far off

An endless volume's **Distance** carries its ground on past its reach, as far as a town across a valley or a coast seen from a ship: in metres each way, and 0 for none. Past the reach it is drawn in rings round the full detail:

- Each ring's voxels are twice the size of the ring's inside it, and it reaches twice as far. At 16 voxels a metre and a reach of 64 m: voxels of 1/8 m to 128 m, 1/4 m to 256 m, 1/2 m to 512 m, then of a metre as far as the Distance.
- Each ring is made on the GPU from the same ground or generator, grass, trees and changes as the volume itself, as it comes into view. Far crowns stay loose cubes in their own shades. Flowers are left out where they would be smaller than a pixel.
- Where one ring meets the next is a square round the camera that moves on with it, so nothing pops as you go. At 1080p with a reach of 64 m, a ring's voxels are about a pixel across where it begins, as the full detail's are drawn two by two there.

With 100 oaks and pines a hectare on rolling hills, at a reach of 64 m on an RTX 3070:

| Distance | On the GPU | Made before the frames | A view across the land at 1080p |
|---|---|---|---|
| None | 122 MB | 1.7 s | 4.7 ms, sky past 64 m |
| 1 km | 500 MB | 3.0 s | 12.7 ms |
| 2 km | 618 MB | 5.4 s | 12.9 ms |

Without trees, rings to 2 km take 433 MB against 94 MB. What a view costs goes with how much ground it shows: the land to the horizon costs what a view full of near ground does.

`vox editor call field.set` sets it as the Inspector does: `{"obj": "Ground", "component": "voxel_volume", "field": "distance", "value": 2048}`. In the scene:

```toml
[obj.voxel_volume]
size = [16.0, 64.0, 16.0]
endless = true
reach = 64.0
distance = 2048.0
```

## Ground from a world generator

An endless volume can take its ground from one of the project's [[Generator Graphs|world generators]]: pick it as its **Generator**. The generator's graph runs on the GPU, a voxel column at a time, and the volume shows that graph's land in small voxels, its hills, valleys and gullies to the voxel. The generator's ground at a place is the CPU's to within a millimetre or so.

The **Generator** can also be a generated world of blocks (a `.blockworld`): the volume then shows the ground its chunks are made of, its generator shaped by its biomes' land, its painted Land, Height and Biome maps read, and the sea or cliffs past a world of a set size. The painted maps go to the GPU with the program, about 9 MB for a world 51 km a side painted all over.

- **Floor**: how high the generator's ground is at the volume's floor, in metres of the generator's world. Until it is set, it is worked out from the generator: the ground at the volume's corner stands two fifths of the way up the volume.
- **Offset**: where the volume's corner lies in the generator's world, in metres along X and Z.
- **Sea**: whether the generator's sea fills the level at its sea level. On unless turned off; see [The sea](#the-sea).

Each column's ground is what a world of blocks with no biomes of its own would lay there, from the generator's height and climate:

- grass where the ground would grow it, with the volume's **Grass** and **Flowers** on it;
- rock where the ground is steeper than 45°, or where the generator's Rock says;
- sand along the shore and in hot dry places, gravel under the sea;
- snow where it is below freezing.

Each has a pattern of its own in a few shades, as a block's face has. The volume's **Ground Height**, **Hills** and **Hill Spacing** are not used then: the hills are the generator's.

With a world of blocks as its **Generator**, each column is instead of the biome the world of blocks puts there: the one painted on its Biome map, else the one its climate is nearest, their borders ragged as the world's are. The ground is that biome's blocks, each in its block's colour: its top block a block deep, its under block as deep as the biome says, then its deep block; its steep block where the ground is steeper than the biome says, its shore block by the sea, and its deep block all the way where the generator's Rock says. A block the biome leaves to the world is the world's own. Grass grows on a biome's ground where its top block is green, in that block's colour, and trees with it. A biome's rock layers are not laid yet.

Most of the generator's nodes run on the GPU: Noise, Math, Curve, Warp, Gullies, Clamp, Blend, Terrace, Height Mask, Cells, Falloff, Landmass, Climate, Peaks, Plateaus (with its rivers' canyons and ramps), Smooth, World Edge and Painted Map, which covers the Battlefield Plains, Rolling Hills, Alpine, Archipelago and Canyons starters and a world of blocks' painted ground. Some are left out there:

- **Rivers** are found on the CPU, as a world of blocks finds them, and laid into the GPU's ground (below).
- A generator's **Volume** runs on the GPU when it is made of Solid, Combine, Noise 3D, Overhangs, Tunnels and Chambers: its overhangs, tunnels and pockets are drawn in voxels, with grass and flowers on ground the overhangs moved, and rock in the walls of caves. A **Cave Network**'s great halls are left out. A Volume with Raised Land, Spires, Fins, Arches or Roof is left out, and the ground is its height made solid.
- A graph with a node that cannot run on the GPU (Raised Land, Spires, Fins, Chasms, Slope Mask) makes the volume's own ground instead, and the Console names the node.
- An endless volume's ground far from its Obj, as a world tens of kilometres across has it, is drawn streaked: put the volume's Obj near the camera and its **Offset** where the camera's ground lies in the generator's world.
- An endless volume is at most 16 of its sectors tall, 64 voxels each: 64 m at 16 voxels a metre, 256 m at 4. Land higher than its top is cut off flat there.

### Rivers and lakes

A generator's **Rivers** node finds its rivers, lakes and waterfalls on the CPU, over the grid of points it works them out from. They are laid into the volume's ground as it is made:

- **The ground is cut for them.** For each 4 m column, the CPU works out how much the rivers lower the ground (or raise it, in the low rim that holds a river in) at points half a metre apart, and how high their water stands. The GPU adds that change, read smoothly between the points, to the height its graph made. Riverbeds are gravel, sand along the water's edge, and nothing grows under the water.
- **The water is drawn**, as voxels of water as it comes (see Water of voxels, below), or smooth, as a world of blocks' rivers and lakes are: still where a lake lies level, its ripples carried downstream where a river flows, a line of foam where it meets the bank, and the reflections and the view of the bed through it. Where a river drops over a cliff, its water leaves the lip and curves down into the pool below, pale and torn into white strands lower down, and churns the pool white where it lands.
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

### Water of voxels

A volume's **Water** sets how its rivers and lakes are drawn:

- **Voxels**, as it comes: the water is voxels as small as the ground's, so it steps where the ground steps and its edges are the voxels' own.
- **Smooth**: a smooth surface over the voxels, as described above.
  - Each voxel is shaded by how deep the water is under it, from sandy where it is shallow to deep green-blue, with ripples, reflections and glints over it, and foam where it meets the bank.
  - Where its top drops a voxel down a sloping river, the step's side is lit as the top, so the river reads as one surface.
  - A waterfall is voxels of water thrown over its lip, each falling as thrown water falls, blue-green as it leaves the lip and whiter the faster it falls. They spread and break up lower down, and some splash back up off the pool, which churns white where they land.
  - Shadows, rays and the sea's floor go through it to the bed.

Water of voxels costs no more than smooth water. A river at 1920 × 1080 on an RTX 3070 is 8.6 ms a frame against 10.5, and a waterfall 8.7 against 9.9. Making a column costs the same either way.

A script sets it with `Voxels.WaterLook = VoxelWaterLook.Smooth`, for every endless volume. `vox editor call field.set` sets it as the Inspector does: `{"obj": "Ground", "component": "voxel_volume", "field": "water", "value": "Smooth"}`. The scene writes `water = "smooth"`, and nothing while it is voxels.

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

### Trees in their World Areas

A kind of tree can keep to [[World Areas]], as a world of blocks' structures do: its **Within** row picks an Area Tag, and the kind grows only inside World Areas with that tag; its **Not Within** row picks one it never grows in. Leave both at None for trees everywhere there is grass. With a wood and an orchard marked out by areas, oaks set **Within** the wood's tag and apple trees **Within** the orchard's grow only there, and a town's area keeps them all out.

- The areas are those outlined on Objs in the volume's dimension, between their Low and High where they have them. Areas painted on a world of blocks' Area map don't reach a volume.
- A kind's **A Hectare** still holds inside its areas: a wood of 60 oaks a hectare has as many as before, just only there.
- Moving an area, or the volume, makes the trees again where it reaches. Collision and `Voxels.Raycast` find the trees where they are drawn.

`vox editor call field.set` sets them as `tree_within` and `tree_not_within`, `#2` for the second kind and so on, picking a tag by its path. A scene can give a kind several tags of each, by their numbers:

```toml
[[obj.voxel_volume.trees]]
plant = "Plants/Apple.voxplant"
per_hectare = 120.0
within = ["…the Orchard tag's number…"]
never_within = ["…the Town tag's number…"]
```

## Crops and ground cover

An endless volume grows crops and ground cover of its own voxels on the ground you choose, in place of its grass: **Add Cover** picks a kind, each drawn its own way.

| Kind | What it is |
|---|---|
| Meadow Grass | Tall grass, scattered, a few blades gone to seed. Flowers still grow among it |
| Wheat | Straw-coloured stalks in rows, each with a golden ear |
| Barley | Paler and shorter than wheat, with longer ears |
| Flax | Thin green stems in rows, some with a blue flower |
| Cabbage | Round heads of leaves in rows, pale at the middle, ringed by darker leaves spread low |

Each kind's rows:

- **Cover**: the kind. None takes it away.
- **On**: the ground it grows on: grass, dirt, rock, sand, snow or gravel, as the volume's ground is there.
- **Soil**: what the ground under it turns to. Crops start as Dirt, so a field shows ploughed earth between its rows; None leaves the ground as it is.
- **Height**: how tall it grows, in metres; for cabbages, how wide a head is.
- **Rows**: how far apart its rows are, in metres, along X. 0 scatters it as grass is.
- **Density**: how much of its ground grows a stalk or a head, from 0 to 1.
- **Within** and **Not Within**: Area Tags, as for trees: wheat set Within a Fields tag grows only in the fields.

The first kind in the list that grows on a spot takes it, so put the crops kept to their fields first and a Meadow Grass for everywhere else last. Ground no kind takes grows the volume's own grass. Cover is drawn, and moves in the wind, but isn't solid: it's walked through, and rays pass it. Cover, and trees kept to two dozen areas, make the ground no slower to make than grass alone.

`vox editor call field.set` sets the rows as the Inspector does: `cover`, `cover_on`, `cover_soil`, `cover_height`, `cover_rows`, `cover_density`, `cover_within` and `cover_not_within` for the first kind, the same with `#2` for the second, and `add_cover`. In the scene:

```toml
[[obj.voxel_volume.cover]]
kind = "wheat"
on = ["grass"]
soil = "dirt"
height = 0.9
rows = 0.25
density = 0.7
within = ["…the Fields tag's number…"]

[[obj.voxel_volume.cover]]
kind = "meadow_grass"
height = 0.55
density = 0.45
```

## Mobs

An endless volume can spawn mobs on its ground round the player while the game plays. **Add Mob** picks a prefab; give it a [[Block Mobs|Block Mob]] and a Character, and it walks the volume's ground.

- **Mob**: the prefab. None takes it away.
- **A Hectare** and **Most**: how many to a hectare round the player, and the most there are at once.
- **On**: the ground they spawn on.
- **Near** and **Far**: they spawn no nearer the player than Near and no further than Far, in metres, on ground no steeper than 45°. Once the player is a fifth again further than Far from one, it is taken away, and another comes nearer.
- **Within** and **Not Within**: Area Tags, as for trees: sheep set Within a Pasture tag spawn only in the pastures.

The player is the game's camera, or the terrain's viewer where a game sets one. A Block Mob on a volume walks straight to where it's sent and wanders about where it was spawned, picking places it can stand. It doesn't yet find a way round what is in its way: one that can't get somewhere within 12 seconds gives up and picks somewhere else. A navigation of voxel ground can be put in the engine through `voxear_engine::voxel_ways`: every agent on a volume walks the ways it finds.

```toml
[[obj.voxel_volume.mobs]]
prefab = "…the prefab's number…"
per_hectare = 3.0
most = 6
on = ["grass"]
near = 20.0
far = 70.0
within = ["…the Pasture tag's number…"]
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

An endless volume's ground can be changed: shapes carved out of it, or filled with a ground (dirt, grass, rock, sand, snow or gravel) or a paving (cobbles, flagstones, bricks, planks or ashlar, dressed stone blocks). A carve takes the ground, its grass and flowers and its trees with it, and opens walls of the layers under the grass. A change goes over the ones before it, so a ball carved into rock just filled takes a bite out of it. The columns a change reaches are made again within a frame or two.

The shapes:

- **Ball**: its voxels' middles within a radius of its middle.
- **Box**: a box turned any way, placed by the middle of its foot. Half a metre tall, it is a slab: a step, or half a block of ground.
- **Ramp**: a box whose top slopes from its foot at its back up to its height at its front: a graded bank, a ramp down to a quay.
- **Area**: an outline of points across the ground, from one height up to another: a street however it bends, a square, a quay, a levelled plot.
- **Cover**: the top of the ground within an outline, as deep as it says, wherever the ground is: a road paved over hills, a field laid. Dug, with no ground, it lowers the ground by as much.

A box, a ramp, an area or a cover also clears the air over its top as far as it says, ground, grass, flowers and trees: the room over a street or a levelled plot. One edit lays a whole street or square, however many voxels it covers.

A paving is laid in a pattern of its own, on the world's grid, so pavings laid side by side join up: cobbles a fifth of a metre across, each row half a stone along from the last; flagstones 0.6 m by 0.45 m; bricks a quarter of a metre long and an eighth tall, each course half a brick along; planks a fifth of a metre wide and two metres long, along X; ashlar half a metre long and a quarter tall. Each stone, brick or board takes one of three shades, with a voxel of joint round it. Any fill can be **tinted**: its colours are shaded from the tint as they were from their own average, so the same bricks can be red, yellow or whitewashed.

From a script, every endless volume a change reaches is changed:

```csharp
// A crater where a shell lands, and a heap of rock where a wall is raised.
Voxels.Carve(landed, 1.5f);
Voxels.Fill(wall, 1f, VoxelMaterial.Rock);

// A cobbled street with three metres of air over it.
Voxels.Area([new(0f, 0f), new(40f, 0f), new(40f, 6f), new(0f, 6f)], 11.75f, 12f, VoxelMaterial.Cobbles, clear: 3f);
// A flagged square laid on the ground's own top, a quarter of a metre deep.
Voxels.Cover(square, 0.25f, VoxelMaterial.Flagstones, clear: 0.5f);
// A step, a bank down to the quay, and a cellar dug under a house.
Voxels.Slab(stepFoot, 2f, 0.5f, VoxelMaterial.Ashlar);
Voxels.Ramp(bankFoot, new Vector3(6f, 2f, 8f), VoxelMaterial.Dirt, turn: 30f);
Voxels.CarveBox(cellarFoot, new Vector3(4f, 2.5f, 5f), turn: 15f);
// A whitewashed brick wall.
Voxels.Box(wallFoot, new Vector3(6f, 2f, 0.5f), VoxelMaterial.Bricks, tint: new Color(0.92f, 0.9f, 0.85f));
```

An outline's points are along X and Z: a `Vector2`'s Y is the world's Z.

`vox editor call` changes them as scripts do, or one Obj's volume alone, each one step to undo. With no `material`, a shape is carved out:

```
vox editor call voxels.carve '{ "at": [12, 24, 9], "radius": 1.5 }'
vox editor call voxels.fill '{ "at": [14, 24, 9], "radius": 1, "material": "rock" }'
vox editor call voxels.box '{ "at": [10, 23.5, 4], "size": [4, 0.5, 2], "turn": 20, "material": "cobbles", "clear": 2 }'
vox editor call voxels.ramp '{ "at": [12, 22, 9], "size": [2, 1.5, 4], "material": "dirt" }'
vox editor call voxels.area '{ "points": [[0, 0], [40, 0], [40, 6], [0, 6]], "low": 23.75, "high": 24, "material": "flagstones", "clear": 3 }'
vox editor call voxels.cover '{ "points": [[0, 0], [8, 0], [8, 8]], "depth": 0.25, "material": "bricks", "tint": [220, 200, 150] }'
```

The volume keeps its changes in order, and its scene saves them, in metres from the volume's corner:

```toml
[[obj.voxel_volume.edits]]
at = [12.0, 24.0, 9.0]
radius = 1.5

[[obj.voxel_volume.edits]]
shape = "area"
points = [[0.0, 0.0], [40.0, 0.0], [40.0, 6.0], [0.0, 6.0]]
low = 23.75
high = 24.0
fill = "cobbles"
clear = 3.0

[[obj.voxel_volume.edits]]
shape = "box"
at = [12.0, 21.0, 9.0]
size = [4.0, 2.5, 5.0]
rotation = [0.0, 15.0, 0.0]
```

A `shape` is `box`, `ramp`, `area` or `cover`, a ball where none is written; `tint` is `[r, g, b]` from 0 to 255.

A column takes up to 4,096 changes that reach it, and a half-metre brick 256; past those the latest are left out.

Trees don't grow where the ground under them was carved, nor where a change filled the voxel over their foot, as a street laid over the ground does.

### The brush

With an endless volume selected, set its **Brush**:

- **Carve** or **Fill**: a ball under the pointer in the Scene view shows where it meets the volume's ground, as big as **Brush Size**. Hold the left button and drag to carve balls out along the way, or fill them with **Brush Ground**; hold Shift to do the other.
- **Box**: drag across the ground to mark out a box's foot, turned **Brush Turn** degrees, its corners on a grid of **Brush Snap** metres (none at 0). Let go and it is filled **Brush Height** tall on the ground where the drag began, clearing **Brush Clear** over it; with Shift it is dug as deep. A Brush Height of 0.5 makes slabs.
- **Area**: click the points of an outline across the ground, and click its first point again to close it: it is laid flat at the first point's height, Brush Height thick, clearing Brush Clear over it, or dug out with Shift.
- **Cover**: an outline clicked as for an area, laying the ground's own top within it, Brush Height deep.

**Brush Tint** shades the fill, white for its own colours. Each stroke, box or outline is one step to undo. The brush is the editor's own setting, kept for every volume and not saved with the scene.

### Digging and building a voxel at a time

A game's digging and building set one voxel at a time: dug out, or placed in a ground or a paving, tinted or not. A voxel set this way goes over everything the volume's edits make, whatever order they came in, and stays until it is set again or let go back to what the edits make.

```csharp
// Dig the voxel the player looks at, and build one on the face beside it.
if (Voxels.Raycast(camera.Position, camera.Forward, out var hit, 6f))
{
    if (Input.Pressed(dig)) Voxels.Dig(hit.Point - hit.Normal * 0.01f);
    if (Input.Pressed(build)) Voxels.Place(hit.Point + hit.Normal * 0.01f, VoxelMaterial.Planks);
}
// Undo a digging there: back to what the ground and edits make.
Voxels.Restore([spot]);
```

`Dig`, `Place` and `Restore` take many points at once, too. A volume keeps up to 255 kinds of placed voxel, a ground or paving and a tint each. In the brush, **Dig** digs the voxel under the pointer and **Build** places one of **Brush Ground** on the face under it, as you drag; Shift does the other. `vox editor call` does the same:

```
vox editor call voxels.dig '{ "points": [[12.03, 23.97, 9.03]] }'
vox editor call voxels.place '{ "points": [[12.03, 24.03, 9.03]], "material": "bricks", "tint": [200, 40, 40] }'
vox editor call voxels.restore '{ "points": [[12.03, 23.97, 9.03]] }'
```

Collision, rays and trees find the voxels set as the GPU draws them: a tree whose foot is dug away grows no more.

### Saved in region files

The voxels set one at a time are kept brick by brick, and saved with the scene in region files beside it, a folder for each volume (`<scene>.voxels/<Obj id>/`) and a file for each region of 64 m a side at 16 voxels a metre: each brick its place, a bit for each of its voxels and a byte for each voxel set, packed with zstd. A tunnel of 320 voxels dug across two regions takes under 2 KB. The scene names the folder and its regions, so it loads them as it loads; a save writes only the regions that changed and takes away those no voxel is set in any more:

```toml
[obj.voxel_volume]
endless = true
changes = "Levels/Harbour.level.voxels/12"
regions = [[0, 0], [-1, 0]]
```

### Colours

A volume holds 4,224 colours: 128 every brick shares (the ground's, the generator's, the pavings' and the trees'), and 32 banks of 128 more. A print's colours, in each tint it is printed in, and a tinted fill's go together in one bank, so a brick of one building has all of its colours; each brick keeps the bank most of its voxels are in. Where two buildings of different banks meet in one brick, half a metre across, the voxels of the other take the shared colour nearest theirs. Sixty kinds of building of thirty colours each, each in a tint of its own, keep every colour.

### Printing models

A model made into a [[Voxel Prints|Voxel Print]] is printed into the ground as another change, kept in order with the rest: a sandbag wall or a crate of voxels, solid all through, which a ball carved later bites into. Set the **Brush** to Print to place one with a click, or call `Voxels.Print` from a script. A print can be fitted to the ground, its inside cleared and a footing under it where the ground falls away, and tinted.

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

A volume is lit as everything else is: by the sun, with the shadows of meshes, blocks and terrain on it, by lamps, by the sky's light and bounced light, with ambient occlusion and reflections, and in the scene's fog.

### Shadows

Its voxels shadow each other, and everything else standing among them: models, characters, doors and ships, blocks, terrain and grass. They are in no shadow map. Once the depth of the picture is drawn, a ray is traced from each pixel towards the sun through the voxels, from whatever is drawn there; what it meets puts the pixel in shadow, as sharp as the voxels are. The Profiler calls it **Voxel shadows**. It reaches 64 m towards the sun.

### Light indoors and underground

Under a roof, in a cellar or a cave, the sky's light is shut out, and what glows lights the room, as a [[Block Light|world of blocks]] is lit:

- **The sky's light** reaches everything the sky is open over, straight down, and spreads from there a step less for each metre, through doors and windows and under eaves. A room with its door shut is dark.
- **Voxels that glow**, a lantern's flame or a lamp, light what is round them in their own colour, as a block's Light does, a step less for each metre: a light of 15 reaches fifteen metres. A glowing voxel shines with its own light too. Which colours of a [[Voxel Prints|Voxel Print]] glow is chosen in its Inspector.
- Models, characters and grass standing in a room are lit as its voxels are, so a figure in a cellar is in the dark too.

The light is kept round the camera on the GPU, a value for each half-metre brick, 48 m across and 24 m tall, and worked out afresh as the voxels change: a few steps of it a frame, all of it when the volumes are new. A brick lets light through as far as lines through its voxels are open, so a wall a few voxels thick in the middle of a brick still keeps it out. Past the light kept, the sky's light is as it is in the open. The Profiler calls it **Voxel light cells**, **Voxel light sky** and **Voxel light spread**: together about 0.3 ms at 1080p on an RTX 3070.

Lamps (Light components) light the voxels as they light anything; their light isn't stopped by voxels yet.

### Glass

A voxel can be see-through, as glass in a window is: what is behind it shows through, a little paler, its own colour over it as much as it is solid, and it reflects the sky and what is round it. The sun shines through it onto the floor of a room, and the light of a room comes in through its windows. Which colours of a Voxel Print are glass is chosen in its Inspector. Only the nearest glass in a pixel is drawn, and far off, where the voxels are drawn two by two or a brick at a time, it is drawn as solid. The Profiler calls it **Glass (voxels)**.

By the sea, a volume's ground is part of the sea floor the waves feel, as meshes standing still are: the waves calm and shorten over its shallows, and foam runs along the shore where they meet it. The floor is found by walking straight down through the voxels round the camera, as far out as the volume reaches.

Far off, where a voxel is smaller than half a pixel, it is drawn as groups of two by two by two, and further still as whole bricks of eight voxels a side in their average colour, so far grass and leaves don't shimmer.

The Profiler's **Voxel coarse rays**, **Depth (voxels)**, **Voxel shadows**, **Opaque (voxels)** and **Glass (voxels)** say what volumes cost. A street of six houses with glass in their windows and a dozen figures among them, on Rolling Hills seen from 20 m up at 1920 × 1080 on an RTX 3070, takes about 0.2 ms for the coarse rays, 2.2 ms for the depth pass, 2.3 ms for the shadows, 0.9 ms to light, 0.1 ms for the glass and 0.3 ms for the light kept round the camera: the whole frame 7.0 ms, against 6.6 ms before voxels shadowed models and lit rooms.

## What it doesn't do yet

- Lamps' light isn't stopped by voxels: a lamp in a room lights the walls outside it too. Glowing voxels are the way to light a room.
- Its voxels don't shadow the scene's fog or its particles, and water and glass meshes don't take their shadows.
- Glass lets the sun through untinted, and only the nearest glass in a pixel is drawn.
- It isn't turned or scaled with its Obj.
- One of a set size is drawn whole, however far away it is. An endless one's rings are made again whenever they come into view, not kept on disk, and its full detail is at most 64 m tall at 16 voxels a metre: taller hills are cut flat near the camera and whole in the rings. Smooth water is laid over the full detail's rivers alone: further out, their beds are dry unless the water is of voxels.
- An endless one made by a generator has no Cave Network halls yet. Its rivers' water doesn't flow into a pit carved beside it. Its sea is under the ground inland too, so a cave dug below the sea's level is under water; turn the volume's Sea off where there is no coast.
- An endless one's trees can't be felled, and every tree of a kind is one of four plants. Its crops can't be harvested, and its mobs don't find ways round what stands in their way. Grass blades short enough to stand wholly in the half-metre brick with their soil keep still in the wind.
- A volume of a set size can't be changed yet, only an endless one. A volume of a set size doesn't collide yet; an endless one does.
- Edits and voxels set one at a time aren't sent over the network yet: every machine makes the scene's own, but not those a game makes as it plays.
