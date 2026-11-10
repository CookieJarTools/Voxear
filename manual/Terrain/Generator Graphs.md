---
order: 3
---

A world generator is a **graph**: nodes that each make a number at every place on the ground, joined by wires that carry what one node makes into another. One **Output** node takes the ground's height and its climate. A new generator's graph is a preset's: a dozen or so nodes that shape one kind of ground, with a few of their values as rows in the Inspector, which is all most worlds need (see [[Generating Terrain]]). The **Generator** panel is where you open the graph, to change it or to build your own.

## Opening a generator's graph

Double-click a World Generator in the Project panel, or press **Open Graph** in its Inspector. The Generator panel opens beside the Scene view, with the whole graph in view. **Show Generator** in the command palette, and the Window menu, bring the panel back.

The panel has the graph on its canvas, and beside it the ground the whole graph makes, in 3D.

## The canvas

| Do this | To |
| --- | --- |
| Turn the wheel | Zoom in or out about the pointer, a step at a time |
| Drag with the middle button, or with Alt held | Move the view |
| Press a node | Pick it. With Shift or Ctrl held, add it to the picked nodes, or take it out |
| Drag a node | Move every picked node. Nodes sit on a grid |
| Drag over empty canvas | Pick the nodes the box touches |
| Drag from a port to a port | Wire them |
| Drag from an input that has a wire | Pull the wire off: let go over another input to move it there, or over empty canvas to take it away |
| Right-click, or press Space | Open the search, to add a node there |
| Delete | Take the picked nodes out, with their wires |
| Ctrl+D | Make another of each picked node beside it |
| Ctrl+C, Ctrl+V | Copy the picked nodes, and put the copies under the pointer |
| F | Show the picked nodes, or the whole graph with none picked |
| Ctrl+Z, Ctrl+Y | Undo and redo |

**Add Node** and **Frame** above the canvas do what Space and F do.

## Nodes and wires

A node's inputs are down its left side and its outputs down its right. An input takes one wire, and a new wire into it replaces the old one. An output can feed as many inputs as you like. A wire cannot lead back round to the node it started from, so a wire that would close a loop does not land.

While you drag a wire, the ports it can go to stay bright and the rest dim. Let go over a node's box rather than a port and the wire takes the node's first free input. Let go over empty canvas and the search opens: the node you pick is added there, already on the wire's end.

A wire takes its colour from what it carries:

| Wire | What it carries |
| --- | --- |
| Field (blue) | A number at each place on the ground: a height in metres, or a mask from 0 to 1 |
| Volume (orange) | A number at each place in space: how far inside the ground it is. A Field can go into a Volume input |
| Biome (green) | One biome |
| Biomes (purple) | Biomes laid over one another |

## Adding a node

The search lists the kinds of node in folders, as Add Component does. Type to search every folder at once; Up and Down move through the list, Enter picks, and Escape closes it.

| Folder | Its nodes |
| --- | --- |
| Sources | Ground from nothing: noise, cells, a falloff, a constant, a whole landmass |
| Shape | What is wired in, changed: a curve, sums, terraces, a clamp, a blend, a warp, smoothing |
| Masks | Where something is, from 0 to 1: by height, slope, curvature or climate |
| Erosion | Gullies cut down the slopes |
| Climate | How warm and how wet each place is |
| | Output, which takes the ground's height, temperature and humidity. A graph needs one |

[[Generator Nodes]] lists every kind of node, with what goes into it, what comes out and its rows.

## A node's values

Pick a node and the Inspector shows its values, one row each, with the ones it seldom needs under **Advanced**. Type in a row, or drag its label left or right to change its number. An angle is typed in degrees. Curves are edited in their rows: drag a key to move it, press where there is none to add one, and drag a key off the graph to take it out.

Some values stand in for an input: a Math node's **A** and **B** are the numbers it uses while nothing is wired into those inputs. Wire something in and the row goes, since the wire is what the node reads.

With no node picked, the Inspector shows the generator itself: press empty canvas to get back to it.

## Exposing a value

Right-click a row's label and choose **Expose**. The value becomes a row of the generator's own, shown in its Inspector under Seed, Scale and Sea level, so it can be changed without opening the graph. A preset's rows, such as its Height and its Gullies, are values of its nodes exposed this way.

An exposed row is named for its value. When another row has that name already, the node's name goes in front, as **Noise Size**. A value that is exposed has its label in the accent colour on its node's rows; **Stop Exposing** in the same menu takes the row away again.

## Previews

Every node shows a picture of what it makes over a square of the world: black where the number is least, white where it is most, with the two numbers under the picture. A node with several outputs shows its first.

Beside the canvas, the ground the whole graph makes shows in 3D over the same square, coloured by height: water, sand, green, rock and snow. Drag it to turn it, and turn the wheel over it to show more of the world or less. It is drawn coarse at once while you change a value, then finer, up to 257 by 257 points, once you stop.

| Setting | What it does |
| --- | --- |
| Center X, Center Z | The middle of the square the previews show, in metres in the generator's world |
| Size | How wide the square is, in metres |

The square is kept in the generator's file, and makes no difference to the ground.

## What is wrong with a graph

A graph with something wrong still makes ground, and says what is wrong. A node at fault has a red edge and a line of red text under it; what is wrong with the graph as a whole shows in the corner of the canvas.

| The graph says | What to do |
| --- | --- |
| No Output node: the ground lies at the sea level. | Add an Output and wire the ground's height into it |
| No height wired in to cool by | A Climate that shapes the ground has nothing in its own Height input. Wire the height it should cool by into it |
| A wire leads back round to it | Only a file changed by hand can hold a loop. Pull the wire off |
| A wire comes from a node that is gone. | Only a file changed by hand names a node it does not have. Wire the input again, or pull its wire off |

## Saving, undo and the level's ground

Each change is saved as you make it. Terrains and block worlds in the open level that use the generator are made again from it when you let go: after a drag ends, not at every step of it, while the previews follow every step. A change that makes no other ground, such as moving a node or exposing a value, makes nothing again.

The graph has its own undo. Ctrl+Z and Ctrl+Y undo and redo changes to the graph while the Generator panel, or the Inspector showing its node or its generator, is where you last pressed; press in the Scene view or the Hierarchy and they are the level's again. Changes close together to one value, or to where nodes are, are one step. The square the previews show is not a step.
