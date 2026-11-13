---
order: 5
---

Plants move in the wind: trees lean and sway, their leaves flutter, and gusts sweep across a field of grass in waves. One **Wind Zone** blows over the whole level, and every plant whose material moves in the wind follows it, in its shading, its shadow and its motion blur alike. Plants also bend out of the way of a player or a vehicle passing through them, and spring back after. The same wind carries smoke, dust and sparks: a Particle System picks up as much of it as its **Wind** says (see [[Particle Systems]]).

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

Only the level's first Global Wind Zone blows over the whole level. Without one, nothing moves on a still day. [[Weather]] has wind of its own, so plants stir in rain and toss in a storm with no Wind Zone at all, and with one it blows harder. A Wind Zone can also blow the sea's wind: see [[The Ocean]].

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

A [[Box Models|box model]] has its own **Wind bend**, in the Inspector while it is open in the Model window, under **Model**. Set it and every one of that plant bends in the wind and away from what walks through it, wherever it stands or grows. Nothing flutters: a plant of cubes leans whole, and what reaches out from over its foot, such as a tree's crown, sways a little on its own.

| Plant | Wind bend |
| --- | --- |
| A tuft of grass | 2 |
| A flower | 1.5 |
| A bush | 0.8 |
| A tree | 0.25 to 0.35 |
| A rock, a character, a chest | 0 |

A low, wide plant bends less for the same number than a tall, thin one, so a tuft wants more than a flower. The starter plants a Block World offers are made with these. One written into a project by an earlier Voxear has none: set its **Wind bend**, or delete its file from the `Plants` folder and pick it on **Add plant** again.

### Blocks that sway

Leaves built of blocks sway too. Turn on a block's **Sways in wind** (see [[Blocks]]), or draw it with a foliage material, and the corners of its faces move a little with the wind, each at its own pace, as gusts cross the crown. A corner that touches a block standing still does not move, so leaves stay fixed to their wood and to the ground, and no gap opens anywhere. A block's corner moves at most about a fifth of a block.

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

Pushing works with or without a Wind Zone, on any plant whose material has a **Wind bend**. Plants are pushed within about 32 metres of the camera, by the 16 pushers nearest it.

In a scene file:

```toml
[obj.foliage_interaction]
radius = 0.6
strength = 1.0
```
