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

## The Model window

Double-click a box model in the Project panel to open it in the **Model** window. You can also show the window from the command palette with **Show Model**.

On the left is a tree of the model's **bones**. Each bone holds its **boxes** and the bones that hang from it. On the right the model stands on a floor marked off in blocks, with each block's pixels showing faintly, so you can see how big a box is.

### Looking around

| Do this | To |
| --- | --- |
| Drag in the view | Turn the camera round the model |
| Drag with the middle button | Move the camera sideways and up |
| Turn the wheel | Move the camera nearer or further |
| Press **F**, or click **Frame** | Look at the whole model |

### Picking

Click a box in the view to pick it, or click a bone or a box in the tree. What is picked is outlined, its row is lit in the tree, and the Inspector shows its numbers. Click the floor to pick nothing; the Inspector then shows the model itself.

### Building

The bar along the top changes the model:

| Button | What it does |
| --- | --- |
| Add Bone | Adds a bone under the bone that is picked, or at the top when nothing is picked |
| Add Box | Adds a small box to the bone that is picked, and gives it room of its own on the sheet |
| Duplicate | Copies what is picked: a box, or a bone with everything under it. **Ctrl+D** does the same |
| Mirror | Copies what is picked to the other side of the body. A bone called `LeftArm` gives one called `RightArm`, and a copied box uses the same pixels turned over |
| Delete | Takes out what is picked. A bone goes with everything under it. The **Delete** key does the same |
| Pivot Only | While this is on, moving a bone moves only the point it turns about, and its boxes stay where they are |

### Bones made for you

You can build a whole model on one bone and let the editor work out its bones afterwards. Click **Make Bones**, and the editor looks at where the boxes are:

- the box that most others touch is the **Body**
- boxes standing on the ground under or beside the body are **legs**
- tall boxes at the body's sides are **arms**, and flat ones are **wings**
- the largest box on top of the body, or in front of it, is the **Head**
- a box behind the body is the **Tail**
- every other box goes to the part it is nearest, so a snout goes with the head

Each part gets a bone that turns where it meets the body: a leg at its hip, an arm at its shoulder, a head at its neck. The bones are named for what they are and where they are, such as `Head`, `LeftArm` or `BackRightLeg`, and nothing moves in the view.

Make Bones replaces the bones the model had. If it guesses a part wrong, undo it, or pick the bone and change its **Role** in the Inspector.

**Add Joint** cuts the picked box in two across its longest side and hangs the far half from a new bone at the cut. Use it on a leg to give it a knee, or on an arm for an elbow. Each half keeps the pixels it showed.

In the tree, drag a bone onto another bone to hang it from that bone, or drag a box onto a bone to give it to that bone. Nothing moves in the view when you do: only what it hangs from changes.

### Moving and sizing

What is picked has three arrows, red for X, green for Y and blue for Z. Drag the end of an arrow to move it along that line. A box also has a small knob on each face. Drag a knob to move that face in or out, which makes the box bigger or smaller.

Everything moves a whole pixel at a time, so boxes always line up with each other's pixels.

### In the Inspector

A picked **bone** shows:

| Row | What it is |
| --- | --- |
| Name | What the bone is called. Press Enter to rename it |
| Role | The part of the body it moves: None, Body, Head, Leg, Arm, Tail or Wing. What animates the model goes by this, not by the bone's name |
| Pivot | The point it turns about, in pixels from its parent's pivot |
| Rotation | How it stands when nothing poses it, in degrees |

A picked **box** shows:

| Row | What it is |
| --- | --- |
| Position | Its lowest corner, in pixels from its bone's pivot |
| Size | How many pixels it measures each way |
| Rotation | How it is turned about its own middle, in degrees |
| Sheet | Where its unfolded faces start on the sheet |
| Mirror | Turns its unfolded faces over from left to right |
| Inflate | How far its faces stand out past its size, in pixels |
| Layer | Whether the box has a layer of raised pixels |
| Height | How far the layer's pixels stand out, in pixels |

### Sculpting with cubes

A plain box is six flat faces. For rounded and stepped shapes, such as hair, ears, hands, leaves or the crown of a tree, make a box of small **cubes** instead and shape it one cube at a time. Each cube is one pixel across and one colour.

Click **Sculpt** in the bar, then press on the model.

| Tool | What it does |
| --- | --- |
| Add | Puts a cube against the face under the pointer, in the colour you have picked |
| Remove | Takes out the cube under the pointer |
| Paint | Colours the cube under the pointer |
| Pick | Takes the colour of the cube under the pointer to sculpt with |

The first time you sculpt on a plain box it becomes a box of cubes that looks the same: as many cubes as it had pixels each way, each taking the colour painted on the nearest face. From then on its shape is its cubes.

Drag to work along a surface. One stroke adds one layer of cubes or takes one layer away, however long you hold it, so you never dig a hole by mistake. A cube added past the edge of the box makes the box bigger to hold it. The face you are over is marked with a white square.

The large square in the bar is the colour you sculpt with, and the small squares are the colours the model's cubes already use. **Symmetry** does the same to the other side of the body.

In **Shape** mode a box of cubes is moved, turned and sized like any box. Making it smaller cuts cubes off. **Voxels** in the Inspector turns a box into cubes, or back into a plain box of the same size, which loses its cubes.

A box of cubes has no faces on the sheet and no raised layer. **Mirror**, **Duplicate** and **Add Joint** work on it as on any box, and everything that animates the model moves it with its bone.

A good way to build a creature in this style is one box of cubes for each part that moves: head, body, each arm and leg. Then **Make Bones** hangs each from its own bone.

### Plants and trees

Flowers, bushes, mushrooms and trees are box models too. Sculpt one, usually on a single bone, and use it like any model:

- Drag it into the Scene view to stand one in your level. On a [[Block Worlds|Block World]] it lands on the blocks.
- Give it to a [[Foliage]] to grow many of it, and paint them onto the ground or onto a Block World's blocks (see [[Painting and Cutting Plants]]). A box model's mesh is `Plants/Oak.boxmodel#mesh0.0` and its material `Plants/Oak.boxmodel#material0`.

### Painting

Click **Paint** in the bar to paint the model's pixels. The bar changes to the painting tools, and the model's **sheet** shows flat beside the model, with the part each box's faces take outlined.

Press on the model to paint the pixel under the pointer, and drag to paint along a line. The pixel you are over is marked with a white square. You can paint on the flat sheet the same way, which is the easy way to reach faces that are hidden on the model. Dragging from the floor, off the model, still turns the camera.

| Tool | What it does |
| --- | --- |
| Pencil | Paints pixels in the colour you have picked |
| Eraser | Makes pixels into holes you can see through |
| Fill | Paints the whole area of one colour under the pointer, no further than the face you pressed |
| Pick | Takes the colour under the pointer to paint with |

The large square beside the tools is the colour you paint with. Click it to open the colour picker, and click it again to put the picker away. The small squares after it are the colours the sheet already uses, most used first. Click one to paint with it again.

**Symmetry** makes a stroke on the model paint the other side of the body as well, so a face or a pattern stays even.

**Lay Out** gives every box room of its own on the sheet and moves its pixels with it. Use it after you have made boxes bigger or added boxes where the sheet was full, when two boxes show the same pixels by mistake. Boxes that share their pixels on purpose, such as a mirrored arm, still share them. The sheet doubles in size when it is too small.

One stroke is one step to undo.

### A raised layer

A box can carry a **layer**: a second set of pixels over its own. Every pixel you paint on the layer stands out from the box as a small block, so hair, a hat brim, a collar or armour has real depth and catches the light, and you still only paint pixels.

1. Pick the box and turn on **Layer** in the Inspector. The layer gets room of its own on the sheet.
2. Click **Paint**, then turn on **Layer** in the bar.
3. Paint on the model. Your strokes now go to the layer of the box under the pointer, and each pixel rises as you paint it. Use the **Eraser** to take pixels off again.

Turn **Layer** off in the bar to paint the box's own pixels again. You can also paint a layer directly on the flat sheet, where its faces are outlined like any others.

**Height** in the Inspector is how far the layer's pixels stand out, in pixels. 1 is a good start. At 0 the layer lies flat on the box, like a sticker.

A layer costs nothing where it has no pixels, and pixels next to each other share their faces. From far away, where single pixels cannot be seen, the engine draws each layer as a flat picture instead, so a crowd of creatures stays cheap.

### Clips made with keys

A box model can carry **clips**: short animations you make yourself, for what [[Mob Motion]] does not do. A wave, a bite, a lid opening.

Click **Animate** in the bar. A timeline opens under the model.

1. Click **New Clip**. A clip starts one second long and loops. Give it a **Name** and a **Length** in the timeline's bar.
2. Click a part of the model to pick its bone. Three rings appear round the bone's pivot, one for each of its axes.
3. Click on the timeline to choose a moment.
4. Drag a ring to turn the bone. That makes a **key**: how the bone stands at that moment. Between two keys the bone goes from one to the other.
5. Click **Play** to watch the clip.

Each bone with keys has a row on the timeline, and each key is a dot on it. Drag a dot to move the key to another moment. Keys stand on steps of 0.05 seconds.

| Button | What it does |
| --- | --- |
| Key | Keys the picked bone as it stands at the moment shown. Use it to hold a pose until a later key |
| Remove Key | Takes away the picked bone's key at the moment shown. The Delete key does the same |
| Loop | The clip goes on from its last key round to its first, so a walk needs no key at its end to match its start |
| Delete Clip | Takes the clip shown out of the model |

With a bone picked, the Inspector has a **Key** section. It shows the bone at the moment shown, and typing in a row keys the bone there:

| Row | What it does |
| --- | --- |
| Rotation | How far the bone is turned from rest, in degrees about X, Y and Z |
| Position | How far its pivot is moved from rest, in pixels |
| Scale | How large the bone and everything under it is. 1 is as built |
| Ease | How the bone goes on to the next key: **Linear** straight there, **Smooth** in a curve through the keys either side, **Step** held until the next key |

Only the bones that move need keys. A bone with none stays at rest.

A box model's clips are clips like any model's (see [[Animation Clips]]). Open the model out in the Project panel to see them, and give one to an [[Animator]] to play it in your level.

Going back to **Shape** puts the model at rest. Renaming a bone keeps its keys.

### Undo and saving

**Ctrl+Z** and **Ctrl+Y** undo and redo changes to the model while the Model window is the one you last clicked in. One drag is one step.

There is nothing to save by hand. The file is written each time you let go, and every Obj drawing the model in your level shows the change straight away.

You can still edit the file as text if you like. Save it, come back to the editor, and the Model window and everything drawing the model show it as the file now has it.

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
| `role` | The part of the body it moves: `body`, `head`, `leg`, `arm`, `tail` or `wing`. Left out for a bone that is none of them |
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

| `layer` | A raised layer over the box: where its faces are on the sheet, as `uv` or `faces`, and its `height` in pixels, 1 when left out |

A box with a size of 0 along one axis is a flat sheet with two sides, good for hair, ears and cloth.

A layer is written under its box:

```toml
[[bones.boxes]]
from = [-4.0, 0.0, -4.0]
size = [8, 8, 8]
uv = [0, 0]

[bones.boxes.layer]
uv = [32, 0]
height = 1.0
```

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

### Cubes

A box sculpted from cubes has `voxels` in place of `uv` or `faces`, and the model has a `palette` its cubes take their colours from:

```toml
palette = ["e25454", "f0c8a0"]

[[bones.boxes]]
from = [-5.0, 0.0, -5.0]
size = [10, 10, 10]
voxels = "…"
```

| Setting | What it does |
| --- | --- |
| `palette` | The colours of the model's cubes, each as `rrggbb`, up to 255. Change one here and every cube of that colour changes |
| `voxels` | The box's cubes, one for each pixel of its `size`, packed as text. Each side is from 1 to 64 |

### Clips

Each `[[clips]]` table is one clip, and each `[[clips.bones]]` table under it holds one bone's keys:

```toml
[[clips]]
name = "Wave"
length = 1.0
looping = true

[[clips.bones]]
bone = "RightArm"
times = [0.0, 0.5]
rotation = [[0.0, 0.0, 0.0], [0.0, 0.0, -120.0]]
```

| Setting | What it does |
| --- | --- |
| `name` | What the clip is called. No two clips share a name |
| `length` | How long it lasts, in seconds, up to 60 |
| `looping` | `true` for a clip that goes round from its last key to its first. Left out, it holds at its ends |
| `bone` | The bone these keys move, by name |
| `times` | When each key is, in seconds from the clip's start |
| `rotation` | For each key, how far the bone is turned from rest, in degrees. Left out when no key turns it |
| `move` | For each key, how far the bone's pivot is moved, in pixels. Left out when no key moves it |
| `scale` | For each key, how large the bone is. Left out when every key leaves it at 1 |
| `ease` | For each key, how the bone goes on to the next: `linear`, `smooth` or `step`. Left out when every key is `linear` |

## Bones and animation

A box model's bones are a skeleton like any animated model's, and every box moves whole with its bone. So what works on a model's bones works here: [[Bone Attachments]] for things held or worn, scripts that read and turn bones (see [[Bones in Scripts]]), and an [[Animator]] playing the model's own clips.

To make one walk, add a [[Mob Motion]] to the Obj that draws it, and a [[Block Mobs|Block Mob]] to have it find its own way through a Block World. It animates the model from how the Obj moves, using the roles of its bones. The Model window's **Motion** mode shows the same motion on the model while you build it.

## Not done yet

- A brush wider than one pixel, and bringing a sheet in from a picture file or saving it as one.
- Sculpting: bringing in a `.vox` file as a box of cubes, filling or carving more than one cube at a time, and making a character from parts picked at random.
- Plants and trees placed by a Block World's generator, by surface and climate. They are placed by hand or painted for now.
- On the timeline: zooming in on a long clip, copying keys, and moving several keys at once.
- Rings that move or size a bone. Position and Scale are typed in the Inspector.
- A cut-out face casts the shadow of the whole face, holes included.
