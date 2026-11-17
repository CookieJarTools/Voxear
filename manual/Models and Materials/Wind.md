---
order: 5
---

Plants move in the wind: trees lean and sway, their leaves flutter, and gusts sweep across a field of grass in waves. A light breeze blows over every level on a fair day, harder in rain and hardest in a storm, and one **Wind Zone** sets the level's wind in full. Every plant whose material moves in the wind follows it, in its shading, its shadow and its motion blur alike. Plants also bend out of the way of a player or a vehicle passing through them, and spring back after. A Wind Zone's wind, and the weather's, carries smoke, dust and sparks too: a Particle System picks up as much of it as its **Wind** says (see [[Particle Systems]]).

## How plants move in the whole game

**Project Settings ▸ Foliage** says how every plant moves (see [[Project Settings]]):

| Setting | What it does |
| --- | --- |
| Wind | How hard the wind blows over a level with no Wind Zone on a fair day, as a Wind Zone's Strength: 0.5, a light breeze, by default; 1 a breeze that sways trees. Rain and storms blow harder on top. 0 keeps plants still until the weather turns |
| Bend | How far every plant bends, in the wind and when pushed, times its own **Wind bend**: 0 holds every plant still |
| Interaction | Whether characters, and Objs with a Foliage Interaction, push plants aside as they pass |
| Push strength | How hard they push, times each one's own **Strength** (under Advanced) |
| Spring back | How many seconds a pushed plant takes to stand most of the way up again (under Advanced) |

The fair day's wind blows along +X, the way clouds drift over a level with no wind. A script changes any of it as the game runs, from the next frame, and the project's settings stay as they are:

```csharp
// Calm in the valley, and nothing tramples the flower beds.
Foliage.Wind = 0.2f;
Foliage.Interaction = false;

// Everything at once.
Foliage.Motion = new FoliageMotion { Wind = 1.5f, Bend = 1.2f, Spring = 2f };

// Which way, and how hard, the wind blows over the plants this moment.
Vector2 blowing = Foliage.Blowing;
```

Under water there is no wind: plants rooted under the sea sway back and forth with its swell instead, slowly, and less the deeper they stand, and under a Pool's still water they drift a little (see [[The Ocean]]).

## A Wind Zone

Add a **Wind Zone** to any Obj (Add Component, then Rendering). The wind blows along the Obj's blue (Z) axis, level with the ground: turn the Obj to turn the wind. While it is selected, the Scene view shows its arrows, longer the harder it blows.

| Setting | What it does |
| --- | --- |
| Strength | How hard it blows: 1 a breeze that sways trees, 3 a gale. 0 stops it |
| Gusts | How much gusts add: 0 for steady wind, 1 for gusts twice as strong as the wind between them |
| Gust size | How wide a gust is across, in metres. Smaller gusts ripple through grass; larger ones roll across a valley |
| Gust speed | How fast gusts travel downwind, in metres a second |
| Turbulence | How much plants sway back and forth and across the wind, rather than lean steadily with it |

Only the level's first Global Wind Zone blows over the whole level, in place of the fair day's wind. [[Weather]] has wind of its own, so plants stir harder in rain and toss in a storm, with a Wind Zone or without, and a storm's wind rises as it nears. A Wind Zone can also blow the sea's wind: see [[The Ocean]].

### Wind over part of the sea

Set **Shape** to **Sphere** or **Box** for a Wind Zone that works only inside it, on the sea's shorter waves: a calm bay in the lee of an island, or a squall out at sea. Inside it the short waves are as rough as its **Strength** says, 0 glassy calm, 1 as the rest of the sea, 2 twice as rough, fading back to the sea's over **Falloff** metres outside it. The long swell rolls through unchanged, as no wind over a small patch of sea can raise or calm it. Plants take no notice of these.

| Setting | What it does |
| --- | --- |
| Shape | Global, over the whole level; or Sphere or Box, over part of the sea |
| Width, Size | A Sphere's width, or a Box's size, in metres, turned with its Obj |
| Falloff | How far outside it its roughness fades back into the sea's, in metres |

In a scene file:

```toml
[[obj]]
id = 40
name = "Wind"
rotation = [0.0, 70.0, 0.0]
[obj.wind_zone]
strength = 1.5
gusts = 0.8
```

## Plants that move

Foliage materials move in the wind on their own: grass, leaves and bushes need no setup. Two material settings say how:

| Setting | What it does |
| --- | --- |
| Wind bend | How much the plant bends from the ground up: nothing at its root, the most at its tip. 1 for foliage, 0 for anything else, unless you set it |
| Leaf flutter | How much its leaves flutter along their faces, quickly and a little, more out at the tips. 1 for foliage, 0 for anything else |

A tree's trunk is usually not foliage, so give it the same **Wind bend** as its leaves, and the leaves stay on their branches as the tree sways. A rock or a building stays at 0.

A plant bends from its pivot, its Obj's position, and reads its height from its mesh. Give plants their pivot at their base, as tree and grass models usually have: a plant whose pivot is in its middle bends only above it.

In a material file, `wind_bend = 1.0` and `wind_flutter = 0.5`.

### Plants of cubes

A [[Box Models|box model]] has its own **Wind bend**, in the Inspector while it is open in the Model window, under **Model**. Set it and every one of that plant bends in the wind and away from what walks through it, wherever it stands or grows. Nothing flutters: a plant of cubes bends whole, each layer of its cubes sliding over the one below as far as its height says, so its faces stay flat and square and nothing parts. It is pushed by what stands at its foot, so a bush leans away from someone walking into it as one. Its faces are made no taller than one of its cubes, so a plant that bends is a little more to draw than one that does not: in a meadow of grass, flowers and bushes, about a tenth of a millisecond a frame on a mid-range GPU.

| Plant | Wind bend |
| --- | --- |
| A tuft of grass | 2 |
| A flower | 1.5 |
| A bush | 0.8 |
| A tree | 0.25 to 0.35 |
| A rock, a character, a chest | 0 |

A low, wide plant bends less for the same number than a tall, thin one, so a tuft wants more than a flower. The starter plants a Block World offers are made with these. One written into a project by an earlier Voxear has none: set its **Wind bend**, or delete its file from the `Plants` folder and pick it on **Add plant** again.

### Blocks that sway

Leaves built of blocks sway too. Turn on a block's **Sways in wind** (see [[Blocks]]), or draw it with a foliage material, and the corners of its faces move a little with the wind, each at its own pace, as gusts cross the crown. They part round someone walking into them, too, as a bush of leaf blocks does. A corner that touches a block standing still does not move, so leaves stay fixed to their wood and to the ground, and no gap opens anywhere. A block's corner moves at most about a fifth of a block in the wind, and under half a block wind and push together.

A block of [[Blocks#Bushy leaves|bushy leaves]] sways its tufts with its corners, a little further, as they reach out past the block. A [[Blocks#Vines|vine]] that hangs free sways from where it is fixed, and one against a wall stays still.

Only the world's own blocks sway, not the larger ones of its far rings. Swaying faces are drawn a block at a time, not joined into larger ones, so a forest of swaying leaves is a little more to draw than a still one.

## Plants from other tools

Without more to go by, a plant bends as one piece and its leaves flutter all over, trunk too. A tree's branches can sway on their own, and only its leaves flutter, when its model has **wind masks**. Set them in the model's Import Settings, under **Wind masks**:

| Choice | What it does |
| --- | --- |
| None | The whole plant bends as one. The model's vertex colours are not read |
| From vertex colours | Masks painted in the model's vertex colours, as CryEngine's and Unreal's plants have them: **red** where leaves flutter, **green** a phase of each leaf's or branch's own, so neighbours move out of step, and **blue** how far a branch sways apart from the trunk |
| Generate | Worked out from the plant's shape, for plants made without masks, as SpeedTree exports and Quixel plants usually are. Each separate piece small beside the whole plant is a leaf or a card of leaves: it flutters, at its own phase, and sways with the branch it hangs from. Everything sways more the further out from the trunk it reaches, and the trunk itself does not flutter |

A branch's sway rides on the plant's own bend, downwind and bobbing up and down, and is as large as the material's **Wind bend** makes it.

## Pushing through foliage

An Obj with a **Character Controller** pushes plants aside by itself: the player and every other character part the grass they walk through, about twice as far round as they are wide, with nothing to set up. Grass and plants lean away, bending from the ground up as they do in the wind, and spring back over about a second, so a trail stays behind someone running through a field.

Add a **Foliage Interaction** to anything else that should push plants: vehicles, a rolling boulder, a creature moved by a script (Add Component, then Rendering). On a character it takes the place of the character's own push: set its size, or set **Strength** to 0 for a character that pushes nothing.

| Setting | What it does |
| --- | --- |
| Radius | How far round the Obj plants are pushed, in metres. About a character's width for a character, wider for a vehicle |
| Strength | How hard: 1 bends grass well over, 0.3 just stirs it |

Pushing works with or without a Wind Zone, on any plant whose material has a **Wind bend**, and on leaf blocks that sway. A pusher bends the plants rooted about as high as it stands, within its radius and a metre and a half above or below it, so a field under a bridge is left alone by whoever crosses. A plant bends as far up as the pusher reaches, and above that it is carried along: a pole taller than someone leans from where they push it. A tall plant is pushed where it stands, so a crown over a path does not part round someone walking under it; leaf blocks are pushed only as high as the pusher reaches. Plants are pushed within about 32 metres of the camera, by the 16 pushers nearest it. **Project Settings ▸ Foliage** turns pushing off for the whole game, or makes it harder or slower to spring back.

## Shadows

A plant's shadow moves with it. The sun's shadow pages are kept from frame to frame with plants drawn in them standing still, and a plant reads its own shadow from where it stands, so the shadows of its leaves on itself move with it and never swim across it. The shadows plants cast on the ground stay where the plants stand, as they move so little. With the sun's shadows drawn as cascades, every frame, plants cast their shadows as they move.

In a scene file:

```toml
[obj.foliage_interaction]
radius = 0.6
strength = 1.0
```
