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
| Starter Blocks | Makes slabs, stairs, a fence, a wall, a pane, a door, a trap door, a gate, a lever, a button, a chain, a rope and a lantern in **Blocks/Starter** (see [Starter blocks](#starter-blocks)) |
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
| Shape | **Cube**, as most blocks are; **Bushy**, a cube with tufts of leaves reaching out of it (see [Bushy leaves](#bushy-leaves)); **Vine**, thin sheets on the blocks beside it (see [Vines](#vines)); or a block of parts: a slab, stairs, a fence, a door and more (see [Blocks of parts](#blocks-of-parts)) |
| Overlap | Where it lies in the order of blocks that meet: its picture creeps over the edge of a block beside it with a lower Overlap. 0 keeps to itself (see [Blocks that meet](#blocks-that-meet)) |
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
- Only a block whose **Shape** is **Cube** joins.

### Blocks that meet

Where two different blocks lie side by side, the edge between them is a straight line unless one of them has an **Overlap**. The block with the higher Overlap creeps over the edge of the other: a ragged fringe of its own picture, a few pixels deep, with teeth of uneven length. Give grass 2, dirt 1 and rock 0, and grass grows over the edge of both, and dirt spills onto rock.

- Nothing needs drawing: the fringe is the block's own picture carried on past its edge, so it suits any picture, and any [[Texture Packs|texture pack]] a player wears.
- A picture drawn with **Sharp pixels**, and a block of a colour alone, creep a whole pixel at a time. A smoothed picture creeps over a soft, wavy edge.
- Blocks meet in one plane: grass creeps over the rock level with it, not over a step up or down, and not over a block whose face is covered.
- Every higher block beside a face creeps over it, each from its own side, the highest on top: rock with grass on one side and dirt on another takes a fringe of each.
- Two blocks with the same Overlap keep a straight edge between them.
- A liquid, a see-through block, one that sways, a bushy block, a vine and a face that joins take no fringe and give none.
- Far off, past the world's own blocks, edges are straight. So are those of a world drawn smooth, which blends its ground its own way.
- A world can mix its blocks while it draws 128 looks or fewer.

### Bushy leaves

Set a leaf block's **Shape** to **Bushy** and its leaves reach out past its cube: three sheets of leaves cross through the block's middle, two upright from corner to corner and one that leans, each nearly twice as wide as the block and thinning out to a ragged edge. Leaves overhang the blocks beside and below, and a crown of such blocks is a soft mass of leaves with no straight edge to it.

- With a material that is cut out, the sheets are drawn with the block's leaf picture, clear where the picture is clear.
- With a colour alone, the sheets and the cube's own faces are drawn as small square leaves in shades of the colour, with dark gaps between them: 16 to a block's side, or as many as the world's **Voxels**.
- A block has its sheets only where it meets open air, so the inside of a crown costs nothing more.
- The sheets sway with the block when it **Sways in wind**, held still wherever the block touches wood or ground.
- They are lit as one soft mass, cast their own ragged shadow, and are drawn out to 160 blocks from the camera, shrinking away before that. The larger blocks of a world's far rings have none.

The block is a cube in every other way: stood on, built on and dug as before, and the sheets are no part of its collision.

In a block's file:

```toml
color = [0.3, 0.62, 0.16]
shape = "bushy"
sways = true
```

### Vines

A block whose **Shape** is **Vine** is a vine: no cube at all, but a thin sheet against each block of ground beside it, a little off its face. With no ground beside it, a vine hangs free as two crossed sheets, from a roof, from leaves or from the vine above it.

- A vine is no ground. Characters walk through it, nothing stands on it, it hides no face behind it and shades no corner, and physics rays pass it by. `Blocks.Raycast` and the brush do meet a vine, so it is placed and broken as any block.
- What a vine clings to is read from the blocks round it, so nothing more is saved for it, and it turns to the wall as soon as one is built beside it.
- With a material that is cut out, each sheet is drawn with the vine's picture, once a block. With a colour alone, a vine is strands running down its whole length with leaves to either side, each strand ending at its own height at the vine's foot.
- A vine that hangs free sways when it **Sways in wind**, fixed where it meets ground. One against a wall stays still.

Vines grow by themselves in a generated world: a [[Biomes#Vines|biome]] hangs them under its ledges and cave roofs, and a [[Structures#Trees of blocks|tree of blocks]] can have them up its trunk and under its crown.

Vines are not climbed yet: a game's own script can ask which block a character is in (`Blocks.DefinitionAt`) and move the character up while it is a vine.

### Blocks of parts

A block whose **Shape** is one of these is drawn as a few boxes in place of a cube:

| Shape | What it is |
| --- | --- |
| Slab | Half a block against one side: lying on the ground, high under a roof, or standing up against a wall |
| Stairs | A step rising away from where it was put. It turns a corner where another stair meets it from the side, and stands upside down in the upper half of its block |
| Fence | A post, with rails to the fences, fence gates and whole blocks beside it |
| Wall | A thick post, with low walls to the walls, fence gates and whole blocks beside it. A straight run has no posts |
| Pane | A thin sheet reaching to the panes and whole blocks beside it, as glass in a window |
| Door | A door against one side of its block, two blocks high, that swings open on its hinge |
| Trap Door | A hatch in the lower or upper half of its block, that swings up on its hinge |
| Fence Gate | A gate between two posts, that fits into a fence and swings open |
| Lever | A handle on a plate, on a floor, a wall or a ceiling, that tips over to turn on and off |
| Button | A button on a floor, a wall or a ceiling, that sinks when pressed and stays on for its **Press Time** |
| Chain | Links hanging from what is over them, swaying a little |
| Rope | A rope hanging from what is over it, swaying a little |
| Boxes | The block's own boxes (see [Boxes of its own](#boxes-of-its-own)) |

How they join what is round them is read from the blocks beside them as their faces are made: a fence reaches to a fence put down beside it, a stair turns a corner when another meets it, and nothing more is saved for either.

- **Faces.** A whole block's face against a block of parts is drawn, but where the parts cover that side whole: the bottom of a slab lying on the ground, the back half of a stair. A block of parts closes no corner in, so the blocks beside it are lit as by open air.
- **Collision.** Physics stands things on the boxes as they are drawn, an open door at its side. Fences, walls and shut fence gates collide a block and a half high, so nothing steps or jumps over them. Characters walk through levers, buttons, chains and ropes, and through doors, gates and trap doors while they are open.
- **Steps.** On a Block World a Character Controller steps up at least half a block by itself, so it walks up slabs and stairs as up a slope.
- **Water.** Water keeps out of blocks of parts, and flows round them.
- **Far off.** In a world's far view, slabs, stairs and walls are drawn as whole blocks, and thinner blocks of parts not at all, nor on its maps.
- **Block Mobs** stand on slabs and stairs, go no further than a fence, a wall or a shut door, and walk through open doors and gates. They open no doors themselves.
- **Their faces** are drawn as the block's materials say, with its pictures. They are not cut into small cubes (see [[Block Worlds#Faces of small cubes|Faces of small cubes]]).

A [[Voxel Terrain]] draws a block of parts as a whole block.

### How a placed block stands

A block of parts keeps how it stands where it is placed: which way it faces, which half of its block it is in, whether it is open, whether it is on, and which side a door's hinge is on. Each shape keeps only what it uses:

| Shape | What it keeps |
| --- | --- |
| Slab | The side it lies against, one of six |
| Stairs | The way it rises, and its half |
| Door | The way it faces, its half (its lower or upper block), whether it is open, and its hinge |
| Trap Door | The side its hinge is on, its half, and whether it is open |
| Fence Gate | The way it faces, and whether it is open |
| Lever, Button | The side it stands out of, one of six, and whether it is on |
| Boxes | The way it faces, and whether it is open |

How a block stands is part of its number in its world: a world numbers each way a block stands that it holds, as it numbers the block itself. So a world, its saved changes and what is sent of them to a player who joins are no larger for it, and opening a door is one block changed.

A block put down with the Build tool stands as a player would put it: a stair rises away from the view, a slab lies on the face clicked, a door stands on the far side of its block with its hinge on the side it was put nearer. **R** turns it on from there (see [[Block Worlds#Building|Building]]). A [[Structures|structure]] places a block standing as its model says, turned as the structure is turned.

### Blocks you use

Doors, trap doors, fence gates, levers and buttons are used by a player. Any other block can be too: check its **Usable** under **Use**, and a game's scripts hear that it was used (see [[Block Worlds#Blocks from scripts|Blocks from scripts]]).

| Block | What using it does |
| --- | --- |
| Door, Trap Door, Fence Gate | Opens it, or shuts it. A door opens whole, both its blocks, and with it the door beside it hung on the other side, so a pair of doors opens as one |
| Lever | Turns it on, or off, and opens or shuts what it is linked to |
| Button | Turns it on for its **Press Time**, a second unless it says, and what it is linked to with it |

A player uses blocks with a **Block User** on its Obj: when its **Use** action is pressed, the block straight ahead of its **Eye** within its **Reach** is used. With no action picked it uses **Player ▸ Interact**, and with no Eye, its own Obj.

| Setting | What it does |
| --- | --- |
| Use | The input action that uses a block |
| Reach | How far ahead a block is used, in metres. 4 unless it says |
| Eye | Where it looks from, and which way: the player's camera. Its own Obj where none is given |

A lever or a button drives the blocks it is **linked** to, as far as 64 blocks off: on, a linked door opens; off, it shuts. Link them with the Build tool's **Link** brush, or from a script. Links are saved with the world's changes.

### Motion

A block that opens or turns on moves there smoothly by its **Motion**: a few keys over time. While it moves, it is drawn turning about its hinge, or sinking, as its keys say, and every player sees it move alike. Its collision is where it ends at once.

| Setting | What it does |
| --- | --- |
| Time | When the key is, in seconds from the start. The last key's time is how long it takes |
| Turn | How far the parts that move are turned about their hinge, in degrees |
| Move | How far they are moved along it, in sixteenths of a block |
| Ease | How the way on to the next key goes: **Smooth**, slow at either end; **Linear**; or **Step**, held and then at once |

Shutting plays it back. A door swings a quarter turn in 0.3 seconds, a trap door in 0.25, a lever's handle tips from one side to the other, and a button sinks a sixteenth: change a key and the block has a motion of its own. A swing a little past a quarter turn and back again makes a door settle into place.

In a block's file:

```toml
shape = "door"

[motion]
keys = [{ time = 0.0, turn = 0.0 }, { time = 0.4, turn = 95.0 }, { time = 0.5, turn = 90.0 }]
```

### Hanging blocks

A chain, a rope, and a block of its own boxes that **Hangs** hang from what is over them, and sway a little: in the level's [[Wind|wind]], and as characters brush past them. A run of them, a lantern under a chain, sways as one from the block that holds it up, more the further down it hangs. **Swing** is the most it swings, in degrees: 3 unless it says. One standing on the ground with nothing over it does not sway.

### Boxes of its own

A block whose **Shape** is **Boxes** is drawn as its own boxes, listed under **Boxes**, up to eight:

| Setting | What it does |
| --- | --- |
| From, To | The box's corners, in sixteenths of a block from the block's corner, as it faces north |
| Moves | Whether the box moves by the block's Motion when it is used |
| Pivot | The point the boxes that move turn about, in sixteenths of a block |
| Axis | The way the axis they turn about runs |

Put down, a block of its own boxes faces the way it was put, turned a quarter at a time. One with boxes that move opens when it is used, as a chest's lid would.

### Starter blocks

**Starter Blocks** in the Blocks panel makes a set of blocks of parts in the project's **Blocks/Starter** folder: an oak slab, stairs, fence, fence gate, door and trap door, a stone slab, stairs, wall and button, a lever, a glass pane, a chain, a rope and a lantern that hangs. The oak, the glass and the chain are drawn with small pictures of their own, made with them: planks, clear glass in a frame, and links. A block of a starter's name already there is left as it is.

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
