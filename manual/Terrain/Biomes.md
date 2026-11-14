---
order: 20
---

A **Biome** is a kind of country: a `.biome` asset that says where it lies, by how warm and wet a place is and how high over the sea, and what the ground there is made of. A generated [[Block Worlds|Block World]] or [[Voxel Terrain]] lists the biomes its ground has, and each place takes the blocks of the biome it is: snow on the heights, sand in the desert, moss in the forest.

## Giving a world biomes

Select the world, pick **Settings**, and set **Ground** to **Generated**. Under the generator's rows is **Biomes**, the list of the world's biomes.

| Control | What it does |
| --- | --- |
| Add Biome | Lists one of the project's biomes, picked from them all. A `.biome` dragged onto the list from the Project panel is listed too |
| Add Starter Biomes | Makes eight ready biomes and their blocks beside the world, and lists them (see [[Biomes#Starter biomes|Starter biomes]]) |
| Add Showcase Biomes | Makes three more, with their blocks and their trees, and lists them (see [[Biomes#Showcase biomes|Showcase biomes]]) |
| A biome's grip | Drags it to another place in the list |
| A biome's cross | Takes it off the list |

Each change makes the ground again, with what you built on it kept, and is one step to undo. The world numbers the blocks its biomes name as it does [[Block Worlds#The blocks a world holds|any block it holds]].

With no biomes listed, the ground is the world's own **Top block**, **Under block** and **Base block** everywhere. With biomes, those three are used wherever a biome names no block of its own.

## Making a biome

Right-click in the Project panel and choose **New Biome**, or run **New Biome** from the command bar, which makes one in the project's **Biomes** folder. A biome is called what its file is. Select it, and the Inspector shows:

### Biome

**Color** is its colour in lists and on maps.

### Climate

| Setting | What it does |
| --- | --- |
| Preset | Sets Temperature and Humidity to one of eight climates at once: Snow, Tundra, Taiga, Grassland, Forest, Desert, Savanna or Rainforest. It says **Custom** once either is anything else |
| Temperature | How warm the places it takes are, from and to, in °C |
| Humidity | How wet, from and to: 0 for dry, 1 for wet |
| Height | Whether it keeps to a height |
| Heights | With Height on, how high over the sea it lies, from and to, in metres |

### Ground

| Setting | What it does |
| --- | --- |
| Top | The block on top |
| Under | The block under the top one |
| Under depth | How many blocks of Under there are |
| Deep | The block below those |
| Steep | The block of steep ground, in place of Top and Under |
| Steep slope | How steep that is, in degrees |
| Shore | The block along the sea, in place of Top |
| Shore height | How far over and under the sea the shore reaches, in metres |

Each block is picked from the project's [[Blocks|blocks]], and the cross beside it takes it out again. A block left out is the world's own there.

### Vines

| Setting | What it does |
| --- | --- |
| Block | The [[Blocks#Vines|vine]] that hangs in the biome: a block whose Shape is Vine. None for no vines |
| Amount | How many of the places a vine could hang from have one, from 0 to 1 |
| Length | How long a vine hangs, in blocks, from and to |
| Depth | How far under the ground's surface vines still hang, in metres: 0 keeps them out of the caves |

A vine starts in open air with ground over it: under a ledge, an overhang, an arch or a cave's roof. It hangs down through the air until it meets something, and clings to any wall it hangs beside. Vines start three times as often beside a wall as out in the open, so most run down rock faces and a few hang free. The same world hangs the same vines every time, and a world without end hangs them as its ground is made.

Only a world's own blocks have vines, not the larger ones of its far rings, and a Voxel Terrain has none.

### Rock Layers

A biome's rock can lie in layers: bands of blocks, one over another, repeating up and down. Cliffs, a canyon's walls and a cave cut through the rock show the bands running level along them, in place of one block from top to bottom. Steep ground shows the layers from its top down; elsewhere they lie under the top and under blocks.

| Setting | What it does |
| --- | --- |
| Tilt | How far the layers lean, in degrees, all one way. A degree or two reads as real rock |
| Wander | How far the layers rise and fall across the land, in metres |
| Layer | A layer's block, picked from the project's blocks. **Add Layer** puts one over the last; the cross takes it out |
| Thickness | How thick that layer is, in metres |

The layers are measured from the sea, the first listed lowest. Thick bands of the main rock with thin ones between read best, as hard rock between softer beds does. A world's far view shows them as far as its blocks are no larger than a band; past that a cliff is its rock's first layer.

### Tints

| Setting | What it does |
| --- | --- |
| Grass | What the colour of blocks whose **Tint** is Grass or Grass Top is multiplied by, red, green and blue, each from 0 to 2 |
| Foliage | The same for blocks whose Tint is Foliage: leaves |

White, 1 for each part, leaves a block as it is; under 1 darkens that part, over 1 brightens it. Its swatch shows it over a pale grey-green, as a block of grass made to be tinted looks.

### Land

A biome shapes its own country, not only the blocks laid over it. **Form** picks the kind, and each comes with the numbers below set for it; change them after.

| Form | What it makes |
| --- | --- |
| Generator's Own | The ground as the world's generator makes it |
| Plains | The ground brought down to a third of its height over the sea, rolling a little |
| Hills | Rounded hills over the ground, with valleys cut down them |
| Mountains | Ranges of sharp peaks with level land between, bands of cliff across their flanks, deep valleys, overhanging faces and natural bridges |
| Tablelands | Level tables at a few heights with sheer walls between, some pillars and bridges |
| Pillars | Low rolling ground with tall towers of rock standing out of it, grown over on top, and bridges between high places |
| Rim | Wide upland plains that end in a sheer wall a few hundred metres over the valley below, a slope of fallen rock at its foot, a lip along its top that hides the drop until you stand at it, and a ramp up it here and there |

| Setting | What it does |
| --- | --- |
| Height | How high its hills, peaks, tables or rolls rise over the generator's ground, in metres |
| Size | How far apart they stand, in metres |
| Valleys | How deep valleys are cut into its land, along winding lines, 0 for none to 1 |
| Overhangs | How far its steep faces lean and hang out, in metres |
| Pillars | How many of the places a pillar could stand have one, 0 to 1 |
| Pillar height | How tall the tallest pillar is, in metres |
| Bridges | How many of the places a natural bridge could span have one, 0 to 1 |

The land rises out of the generator's ground over the first 40 m above the sea, so coasts and islands stay where the generator put them. Where one biome meets another, its land fades into the next over a few degrees of the climate past its own range, so mountains fall away into hills rather than stopping at a line. Where a place is within the ranges of several biomes, the narrowest one's land is what it has, as its blocks are. A pillar or a bridge stands whole where its biome is, never cut off at the border. Rivers and lakes run down the land so shaped.

A world's **Land from** (see [[Block Worlds]]) keeps the generator's own ground round where the world starts, so a level built there keeps the ground it was built on.

The land is made of the generator's own nodes (Peaks, Plateaus, Noise, Spires, Arches and Overhangs, each kept to its biome by a Climate Mask and blended in), added to a copy of its graph when the world is made. The same shapes can be built by hand in a [[Generator Graphs|generator graph]] and changed there.

A biome's land is worked out only where that biome is: a generator passes over whatever a Blend would leave out, wherever its mask leaves out the same side for a whole batch of places at once. Within **Land from** none of it is worked out at all. So land costs where it lies, and ground near the start costs what its generator's does.

Each change is written to the biome's file as you make it, and every world listing the biome has its ground made again at once, in the level you are editing and in a game you are playing. There is nothing to save, and nothing to undo.

## Tints

A block whose **Tint** says so (see [[Blocks]]) takes its colour from the biome it stands in, so one block of grass is fresh green in a meadow, deep green in a forest and straw yellow on a savanna, and one block of leaves greener in the rainforest than in the taiga. Where two or more biomes meet, the colour fades from one biome's to the next over a few dozen blocks, never stopping at a line, however ragged the border between their blocks is.

How it fades: the world keeps its tints on a lattice, a point every 4 blocks. Each point takes the tints of the biome there averaged with those of the points 3 each way round it, so 28 blocks across, and each face reads between the four points round it. A point's tints come from where it is alone, so the colour is the same on every machine and in every chunk.

Tints are kept round where the world is seen from, a little over 500 blocks each way, worked out a few tiles a frame, the nearest first; past that, a face takes the tint at their edge. A world drawn smooth is untinted. A block's colour is its own wherever its world has no biomes, or its biomes no tints.

## Biome regions

A world can keep its biomes to regions: one biome across a whole region, a few hundred metres to a couple of kilometres across, until you cross into the next. There are no specks or thin bands of another biome inside a region, as there are where every place takes the biome its own climate is nearest. Set a Block World's **Biome size** (see [[Block Worlds]]) to how far apart the regions are; nothing turns them off. A world given generated ground in the editor starts with regions 768 m apart.

- **Where the regions lie.** The world is shared out among points about the Biome size apart, each moved a little by the seed. Every place belongs to the nearest, measured after a smooth warp. Borders wander and bend rather than running straight, and no region is ever broken in two.
- **Which biome a region is.** The one its climate is nearest, read once at its middle at the sea's height, by the biomes' usual rules. A cold region is cold country through, a dry one dry. Neighbouring regions of the same biome read as one large one.
- **Snow and other biomes that keep to a height or sunk ground** are not regions of their own. They lie over whichever region they are in, where a place's own climate, height and sinking are within all their ranges, as before: snow on the mountains of any cold region, a chasm's floor wherever there is one.
- **In a world of [[Block Worlds#Levels|levels]]** the regions are its areas, as wide as its Area size, so each area at its height is one biome, and the area below a wall can be another. Snow counts its height from the area's floor.
- **At the border** blocks change along a ragged line. Grass and leaves fade from one biome's tint to the next over a few dozen blocks, and each biome's [land](#land) blends into the next over about 96 m. So one kind of land goes on until the next, as one biome does.

## How a place picks its biome

A world's generator gives every place a temperature and a humidity (see [[Generating Terrain#Climate|Climate]]), 6.5 °C colder for every 1000 m up. A place is the biome whose Temperature, Humidity and Heights it is within.

- Within none, it is the nearest.
- Within several, a biome that keeps to a height comes first, so peaks stand over whatever grows below them. After that the narrowest wins: an oasis listed with a desert shows inside it.
- Between two that are the same, the one listed first has it.

Borders wander a little, so none runs straight or follows a line of equal warmth.

How wide a world's warm and cold, wet and dry places are is its generator's **Climate size**. Make it smaller for more kinds of country close together.

## Starter biomes

**Add Starter Biomes** makes eight biomes in a **Biomes** folder beside the world, and the blocks they need, each a plain colour, in its **Blocks** folder.

| Biome | Climate | Top, over | Steep | Shore |
| --- | --- | --- | --- | --- |
| Snow | Above 260 m, up to 14 °C | Snow, over Ice | Stone | Gravel |
| Tundra | Up to 4 °C, dry | Frozen Dirt | Stone | Gravel |
| Taiga | -2 to 10 °C, wet | Needles Floor, over Dirt | Stone | Gravel |
| Grassland | 4 to 22 °C, middling | Grass, fresh green, over Dirt | Stone | Sand |
| Forest | 6 to 22 °C, wet | Grass, deep green, over Dirt | Stone | Gravel |
| Desert | From 18 °C, dry | Sand, over Sandstone | Sandstone | Sand |
| Savanna | From 20 °C, middling | Grass, straw yellow, over Red Earth | Stone | Sand |
| Rainforest | From 20 °C, wet | Grass, lush green, over Mud | Stone | Sand |

The Grassland, the Forest, the Savanna and the Rainforest are one block of **Grass**, its Tint Grass, each biome tinting it its own colour, so they fade into one another where they meet (see [Tints](#tints)). Their foliage tints colour the starter trees' leaves the same way, and the Taiga's its pines'.

All but the Desert have Stone deep down. Ground steeper than 45° is rock; snow lies on ground up to 58°.

Each has [land](#land) of its own: plains in the Tundra and the Savanna, upland plains ending in a sheer rim over the valley below in the Grassland, hills in the Taiga and the Forest, tablelands in the Desert and pillars of rock in the Rainforest. The Snow keeps the ground it lies on.

Their rock lies in layers: Stone with thin bands of Gravel, and the Desert's Sandstone with Red Earth between.

The wet ones have [[Blocks#Vines|vines]], of a **Vine** block made with the rest: thick in the Rainforest, some in the Forest, a few in the Taiga and the Grassland, hanging under ledges and cave roofs down to 40 m under the ground.

A block of the same name anywhere in the project is used as it is, and a biome of the same name already in the folder is listed as it is, so nothing you made is written over and the button is safe to press twice.

The Snow keeps to a height, since cooling alone puts snow only on very high ground: a world 12 °C at the sea freezes at 2000 m. So mountains have snow on their tops from 260 m, higher in a warm place, and low ground has none. For a cold world with snow on its low ground too, switch **Height** off on its Snow.

They are a start. Change their blocks, their ranges and their colours, or copy one and make another kind of country of it.

## Showcase biomes

A world gets three showcase biomes when it is first given a **Generator**, if it lists no biomes yet. They show off what a biome can be. **Add Showcase Biomes** lists them on a world that already has a generator.

| Biome | Climate | Top, over | Steep | Shore | Trees |
| --- | --- | --- | --- | --- | --- |
| Autumn Woods | The coolest third | Grass, golden brown, over Dirt | Stone | Gravel | Maple, red |
| Blossom Grove | The middle third | Grass, bright green, over Dirt | Stone | Sand | Blossom, pink |
| Volcanic Fields | The warmest third | Ash, over Basalt | Magma Rock, from 50° | Obsidian | None |

They keep to warmth alone. When they are made for a world, its generator's warmth over the land within a kilometre of where the world starts is split into thirds, and each biome takes one. So all three lie a short walk apart in a world of any climate, cold mountains and tropical islands alike, usually within a few hundred metres of the start. The splits are written into their `.biome` files, so change them there. A world with no generator yet parts them at 10 and 16 °C.

The Autumn Woods are [mountains](#land) with bands of cliff, deep valleys and natural bridges; the Blossom Grove stands among pillars of rock, its trees on their tops; the Volcanic Fields are tablelands.

**Magma Rock** is hot, 220 °C out to 2 m (see [[Temperature#Hot and cold blocks|Hot and cold blocks]]), so food dropped on a volcanic slope cooks. The grove and the woods have a few vines.

Their trees are made as the [[Block Worlds#Trees of blocks|starter trees]] are: a `.voxplant` in the Plants folder and a `.structure` in the Structures folder, listed under the world's generator, each kept to its own biome. Take the biomes off and their trees go with them. The biomes and blocks are made beside the world, and anything of the same name is used as it is, as with the starters.

## Biomes and covers

Before biomes, a block could cover generated ground by its climate, with **Cover** under a world's Build tool. Biomes replace covers. A world that lists biomes shows no Cover to set, and a world that has covers and lists no biomes keeps them, working as they did.

## Biomes from scripts

**Biomes.At** says which biome a place is.

```csharp
public Biome Desert;

var biome = Biomes.At(player.Position);
label.Text = biome.Name;
if (biome == Desert)
{
    thirst.Rate = 2f;
}
```

| Member | What it gives |
| --- | --- |
| `Biomes.At(position)` | The biome the place is, by its ground, however high over it or deep under it the place is. `Biome.None` where no world with biomes reaches |
| `biome.Name`, `biome.Color` | What it is called and its colour |
| `biome.IsValid` | False for `Biome.None` |
| `Biome.Load(path)` | A biome by its path, for one no field holds |

A `Biome` field on a behaviour is picked in the Inspector, as a `Block` is.

## Biomes in a build

A biome ships with a world that lists it or a field that names it, with the blocks it names.
