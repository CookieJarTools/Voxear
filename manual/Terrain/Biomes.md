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

Each change is written to the biome's file as you make it, and every world listing the biome has its ground made again at once, in the level you are editing and in a game you are playing. There is nothing to save, and nothing to undo.

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
| Grassland | 4 to 22 °C, middling | Grass, over Dirt | Stone | Sand |
| Forest | 6 to 22 °C, wet | Moss, over Dirt | Stone | Gravel |
| Desert | From 18 °C, dry | Sand, over Sandstone | Sandstone | Sand |
| Savanna | From 20 °C, middling | Dry Grass, over Red Earth | Stone | Sand |
| Rainforest | From 20 °C, wet | Jungle Floor, over Mud | Stone | Sand |

All but the Desert have Stone deep down. Ground steeper than 45° is rock; snow lies on ground up to 58°.

A block of the same name anywhere in the project is used as it is, and a biome of the same name already in the folder is listed as it is, so nothing you made is written over and the button is safe to press twice.

The Snow keeps to a height, since cooling alone puts snow only on very high ground: a world 12 °C at the sea freezes at 2000 m. So mountains have snow on their tops from 260 m, higher in a warm place, and low ground has none. For a cold world with snow on its low ground too, switch **Height** off on its Snow.

They are a start. Change their blocks, their ranges and their colours, or copy one and make another kind of country of it.

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
