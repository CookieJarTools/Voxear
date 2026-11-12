---
order: 18
---

Things a world's generator builds where it finds ground for them: a house, a well, a ruin, a tree, a whole town. Each is a `.structure` file, listed on a generated [[Block Worlds|Block World]] or [[Voxel Terrain]] under its generator. A structure is built the same from whichever chunk reaches it first, so a house standing across two chunks is whole, and every player sees the same world.

A structure is built of one of three things:

- **A model**: its own blocks, kept in its file.
- **A plant**: a tree or a bush grown from a `.voxplant` afresh at every place, so no two trees of a wood are alike.
- **A town**: houses and a middle, each a structure with a model, laid out along streets (see [Towns](#towns)).

## Where structures stand

Every structure has a placement. The world is cut into squares **Spacing** apart, and each square has one try at a place within it. Its rules decide which tries are built:

| Rule | What it does |
| --- | --- |
| Spacing | How far apart its tries are, in metres. Larger is rarer |
| Separation | The least between two of its own places, in metres, under its spacing |
| Chance | How many of its tries are built, from 0 for none to 1 for all |
| Apart | The least between one of its places and another structure's, in metres. Where two structures both keep a distance, the larger is kept |
| On | Land, Water or Underground |
| Height | How high over the sea its ground may be, from and to, in metres. Left out for any |
| Slope | The steepest ground it stands on, in degrees |
| Biomes | The biomes it stands in (see [[Surfaces#Biomes|Biomes]]). None for any |

Spacing, Chance and Apart together keep a world from being crowded: a large town with a spacing of 2 km, a chance of 0.5 and Apart of 600 m comes rarely and alone, while a well with a spacing of 200 m is common.

Structures are built in the order the world lists them. One listed later keeps clear of what earlier ones built: its trunk off their streets, its crown off their houses. So list towns first, then lone buildings, then woods.

## How the ground meets a structure

| Ground | What it does |
| --- | --- |
| Leave | The ground as it is: for trees and standing stones |
| Foundation | Ground built up under its lowest blocks, so nothing stands on air on a slope |
| Flatten | A foundation, and the hillside cleared from over its footprint as high as it stands |

**Sink** stands it that many blocks into the ground.

## Towns

A town grows as a village does. Streets run out from its middle and take the way that climbs least, so they bend round hills rather than over them, and now and then a shorter street branches off. They stop at ground too steep to walk, at the sea, and near another street. Houses stand along both sides, set back a block from the street, turned to face it, with a short path from the door. They are built nearest the middle first, so a town is thickest there and thins out along its streets.

| Setting | What it does |
| --- | --- |
| Middle | The structure at its middle: a well, a square, a hall |
| Houses | The kinds of house it builds (see below) |
| Count | How many houses, from and to |
| Radius | How far its main streets run, in blocks, from and to |
| Path, Path width | The block its streets are made of, and how wide they are |
| Wander | How much its streets bend, from 0 for straight |
| Branching | How often its streets branch, from 0 for never |
| Roads | Whether it has roads to the next towns of its kind. Off unless switched on |
| Road, Road width, Bridge | The block of its roads, how wide they are, and the block of the bridges where they cross water |
| Link | How many of the roads to its nearest towns are made, from 0 to 1 |

A house's door is in the middle of the side of its model that faces -z.

Each kind of house has rules of its own, so a town can have many cottages, one chapel near its middle and barns out at its edge:

| Rule | What it does |
| --- | --- |
| Weight | How often it is chosen against the others |
| Most | At most how many a town has. 0 for no limit |
| Zone | How far out from the middle it stands, as a share of the town's reach, from and to: 0 to 0.3 near the middle, 0.6 to 1 at the edge |
| Biomes | The biomes it stands in, so a town spread over two biomes builds each its own way. None for any |

A small hamlet and a large town are two structures, each with a town of its own size and a placement of its own: the hamlet common, the town rarer and kept further apart from the rest.

### Roads between towns

With **Roads** on, a town is joined to the towns of its kind next to it. A road leaves one town and heads for the next, curving round hills where it is far from either and heading straight in as it nears a town. It crosses water on a bridge a block over the water, with posts down to the bed. Which towns are joined is decided by the two towns alone, so a road is the same from every chunk it crosses.

## Not yet

- Structures are built near the camera only. Far off, past a world's own chunks, they do not show.
- Structures and towns are made by writing their files. The editor has no tools for them yet.
