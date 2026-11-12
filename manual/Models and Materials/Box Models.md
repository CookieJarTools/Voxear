---
order: 9
---

A **Box Model** is a model built of boxes, with pixels on every face, made for the blocky look of [[Block Worlds]]: animals, monsters, villagers, tools. Its boxes hang from **bones**, so a leg or a head can turn, and it keeps its own pixels, so one `.boxmodel` file is the whole model.

To the rest of the engine a box model is a model like any other. You place it, draw it with a Model Renderer, attach things to its bones and build it into your game the same way as a `.glb`; see [[Models]].

## Making a box model

Right-click in the Project panel and choose **New Box Model**. You get a grey block one block across, standing on the ground, with one bone called `Body`.

Select it to see it in the Inspector. Drag the preview to turn it. The **Box Model** section shows how it is measured and what it is made of:

| Row | What it shows |
| --- | --- |
| Scale | How many of the model's pixels make one metre, the side of a block at its usual size |
| Sheet | The size of the picture its faces are cut from, in pixels |
| Bones | How many bones it has |
| Boxes | How many boxes it has |

Drag the file into the Scene view or the Hierarchy to place it, as you would any model.

The window for building and painting box models in the editor is not there yet (see **Not done yet** at the end of this page). Until it is, a box model is edited as text: open the `.boxmodel` file in a text editor, save, and come back to the editor. Everything drawing the model shows the change.

## How a box model is measured

Everything in a box model is measured in its own **pixels**. With 16 pixels per block, a box 8 pixels wide is half a block wide, and each pixel on its faces is a square one sixteenth of a block across. Keeping every model at the same pixels per block makes them all look as if they belong to the same world.

The model looks along +Z. Its **front** faces are the ones at +Z, its **right** is +X, and its feet stand at height 0.

Pixels are drawn sharp: each one is a square of a single colour however close the camera is. A pixel less than half solid is a hole you can see through.

## The file

A `.boxmodel` file is text:

```toml
pixels_per_block = 16
sheet = [64, 32]
pixels = "…"

[[bones]]
name = "Body"
pivot = [0.0, 12.0, 0.0]

[[bones.boxes]]
from = [-4.0, 0.0, -2.0]
size = [8, 12, 4]
uv = [16, 16]

[[bones]]
name = "Head"
parent = "Body"
pivot = [0.0, 12.0, 0.0]

[[bones.boxes]]
from = [-4.0, 0.0, -4.0]
size = [8, 8, 8]
uv = [0, 0]
```

### The sheet

| Setting | What it does |
| --- | --- |
| `pixels_per_block` | How many pixels make a metre. 16 when left out |
| `sheet` | The sheet's width and height in pixels, up to 1024 each |
| `pixels` | The sheet's pixels, packed small. Left out, the sheet is white |

The pixels are written by the engine, not by hand. A sheet with 256 colours or fewer is kept as a list of its colours and one byte for each pixel, then compressed, so a whole character's pixels are a line or two of text.

### Bones

Each `[[bones]]` table is one bone. A bone has to come after its parent in the file.

| Setting | What it does |
| --- | --- |
| `name` | What the bone is called. No two bones share a name |
| `parent` | The bone it hangs from, by name. Left out, the bone hangs from the model itself |
| `pivot` | The point the bone turns about, in pixels from its parent's pivot, or from the model's feet when it has no parent |
| `rotation` | How the bone stands when nothing poses it, in degrees about X, Y and Z |

Put a pivot where the joint is: a leg's at the hip, a head's where the neck meets the body. Moving a bone moves its boxes and every bone under it.

### Boxes

Each `[[bones.boxes]]` table under a bone is one box carried by that bone.

| Setting | What it does |
| --- | --- |
| `from` | The box's lowest corner, in pixels from its bone's pivot |
| `size` | How many pixels it measures along X, Y and Z |
| `uv` | Where its faces are on the sheet, laid out as the box unfolded (see below) |
| `mirror` | Turns the unfolded layout over from left to right, so a left arm can share the right arm's pixels |
| `faces` | A place on the sheet for each face, in place of `uv` |
| `inflate` | How many pixels each face stands out past the box's size. Its pixels stretch to fit. For clothes over a body |
| `rotation` | How the box is turned about its own middle, in degrees |

A box with a size of 0 along one axis is a flat sheet with two sides, good for hair, ears and cloth.

### A box unfolded on the sheet

With `uv`, a box's six faces sit together on the sheet as if the box were cut open and laid flat. `uv` is the upper left corner of the whole layout. For a box `w` wide, `h` high and `d` deep:

```text
        ┌───────┬────────┐
        │  top  │ bottom │   d pixels high
┌───────┼───────┼────────┼───────┐
│ right │ front │  left  │ back  │   h pixels high
└───────┴───────┴────────┴───────┘
    d       w        d       w
```

The layout is `2 × (w + d)` pixels wide and `d + h` high. The four sides run round the box in order, so pixels next to each other on the sheet are next to each other on the model.

### A place for each face

With `faces`, each face has its own rectangle on the sheet: its left, top, right and bottom edges in pixels. A face you leave out is not drawn.

```toml
[[bones.boxes]]
from = [-4.0, 0.0, 0.0]
size = [8, 8, 0]
faces = { front = [0, 0, 8, 8], back = [8, 0, 16, 8] }
```

Writing the right edge before the left, or the bottom before the top, draws the face turned over.

## Bones and animation

A box model's bones are a skeleton like any animated model's, and every box moves whole with its bone. So what works on a model's bones works here: [[Bone Attachments]] for things held or worn, and scripts that read and turn bones (see [[Bones in Scripts]]). A box model has no clips of its own yet.

## Not done yet

- The **Model** window for building box models in the editor: adding and moving boxes, painting pixels, and making bones from the model's shape.
- Motion made for you from how a model moves: walking, idling, looking, attacking.
- A raised second layer, where painted pixels stand out from the surface as small blocks.
- Animation clips made on a timeline.
- A component that walks a box model through a Block World.
- A cut-out face casts the shadow of the whole face, holes included.
