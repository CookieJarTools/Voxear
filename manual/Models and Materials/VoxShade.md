---
order: 8
---

**VoxShade** makes custom shaders out of nodes rather than written in Slang. You wire nodes together on a canvas, and the editor turns them into the same kind of shader a `.shader` file holds. Materials pick a VoxShade shader as their **Shader** and set its properties in the Inspector, exactly as they would for one written by hand (see [[Custom Shaders]]).

A VoxShade shader costs what the same shader written by hand costs. Before any Slang is written, the editor works out everything it can: numbers known when the graph is saved, work that changes nothing (adding nothing, multiplying by one), the same work done by two nodes, and nodes nothing reaches. Work on a material's properties alone, as a tint made brighter or a direction made a unit long, is done once for each material on the CPU, when its properties are set, rather than at every pixel or vertex. Hologram and Sway made again as graphs draw the same pictures as the hand-written ones, in the same time on the graphics card.

## Making a VoxShade shader

Right-click in the Project panel, choose **New Shader…**, and pick **VoxShade**. It starts as a tint over a picture, lit as the Standard shader is. **Hologram (VoxShade)** and **Sway (VoxShade)** start as the built-in shaders made of nodes, to change. **Sub-graph** makes nodes for other VoxShade graphs to use as one (see [Sub-graphs](#sub-graphs)), and **Particle Graph** a graph that moves and draws Particle Systems' particles (see [Particle graphs](#particle-graphs)).

Double-click the `.voxshade` file, or press **Open** in its Inspector, to open it in the **VoxShade** panel.

## The VoxShade panel

The graph fills the middle. Down the side are the picked node's values, the shader's properties and its own settings. The bar along the top shows what each stage of the shader costs.

- **Add a node:** right-click on the canvas, press **Space**, or click **Add Node**, then type to search. The nodes are in folders: Inputs, Values, Properties, Math, Logic, Vectors, Noise and Outputs.
- **Wire nodes:** drag from an output on a node's right to an input on another's left. Let go over empty canvas and the search opens, to add a node on the end of the wire. Drag from a wired input to pull its wire off.
- **Pick and move:** click a node to pick it. Shift-click or Ctrl-click to pick more, or drag a box round them. Drag picked nodes to move them.
- **Look round:** the mouse wheel zooms. Drag with the middle button, or with Alt held, to move the view. **F** or **Frame** fits the whole graph in view.
- **Delete**, **Ctrl+D** duplicates, **Ctrl+C** and **Ctrl+V** copy and paste, with the wires between the nodes copied. **Ctrl+Z** and **Ctrl+Y** undo and redo while the panel is the one you last clicked in.

Each change is saved as you make it, and the shader compiles into the engine's own in the background, as a `.shader` does when it is saved. Every material using it changes a few seconds later.

### What a node makes

Each node shows what it makes on a small ball: a number in grey, two numbers as red and green, three or four as a colour. The Surface Output shows the ball lit by the colour and glow it gives. Properties show their defaults, and pictures read as white, as they do for a material that has not picked one. A node that reads the time moves while the panel is open. The previews are worked out on the CPU from the same steps the shader compiles to, so they show what the graphics card will work out.

### What a node costs

Under each node is what it costs once the graph is compiled: how many instructions it takes, roughly, and how many pictures it reads. A node with nothing under it costs nothing, because it was worked out when the graph was saved, its work is shared with another node, or it reaches nothing. **Once a material** under a node means its work reads only properties, so it is done on the CPU when a material's properties are set, and the graphics card reads the answer. A shader has room for 16 values: its properties take theirs first, and the rest hold what is worked out once, the costliest work first. The bar adds up each stage: **Surface** runs once a pixel, **Vertex** once a vertex and **Light** once a pixel for each light.

A node that stops the graph compiling says what is wrong on the node itself, as a Light Color in the Surface stage, which has no light.

### A node's values

Pick a node and its values show at the top of the side:

- What the node does, where it can do more than one thing, as which sum a Math node does or which input an Input node gives.
- The numbers each input takes with nothing wired into it. A Multiply with nothing in **B** multiplies by 1; type another number to multiply by that.
- For an output node, an input with nothing wired and nothing typed keeps the material's own: leave **Roughness** empty and the material's Roughness is used. Empty a typed value to go back to the material's.

## The nodes

| Folder | Nodes |
| --- | --- |
| Inputs | UV, Position, Normal, View Direction, Object Position, Level Position, Time, Thickness, and in the Light stage Light Direction, Light Color and Light View. **Surface** gives the surface as it stands: the material's own values, or in the Light stage what the Surface stage set |
| Values | Number, Vector 2, Vector 3, Vector 4, and Color, picked as it looks on screen |
| Properties | Each of the shader's properties, and **Sample** for each picture property. A picture property's own node gives the picture, to wire into a Sample or a sub-graph |
| Pictures | **Sample** reads the picture wired into it at the UV wired in |
| Math | Add, Subtract, Multiply, Divide, Minimum, Maximum, Power, Negate, Absolute, Floor, Ceiling, Fraction, Sine, Cosine, Square Root, Exponential, Saturate, One Minus, Sign, Normalize, Length, Distance, Dot, Cross, Reflect, Step, Lerp, Clamp, Smoothstep |
| Logic | **Compare** gives true or false; **Branch** gives one value or another as a condition holds |
| Vectors | **Split** a vector into its numbers, **Combine** numbers into one, **Swizzle** its numbers into another order |
| Noise | **Random**: a number from 0 to 1 that looks random and is the same for the same seed |
| Custom | **Slang**: code of your own (see [Slang nodes](#slang-nodes)) |
| Sub-graphs | Each of the project's sub-graphs |
| Outputs | **Surface Output** sets the surface's colour, alpha, normal, metallic, roughness and emission. **Vertex Output** moves each vertex. **Light Output** replaces the standard lighting with your own. In a particle graph, **Update Output** and **Look Output** instead |

Most Math nodes work on numbers of any width: wire a Vector 3 into an Add and it adds Vector 3s. A single number wired in beside a vector stands for that number in every place.

A **Branch** worked out once a draw, as on a property, only runs the side it takes, when that side is worth skipping. One that changes from pixel to pixel runs both sides unless one reads a picture or does a lot of work, because pixels running together wait for each other either way.

## Slang nodes

A **Slang** node runs code you write, for what the other nodes cannot do well, as a loop. Pick it to set its values beside the canvas:

- **Name**: what its function is called in the shader's Slang.
- **Input** and **Output** rows: each a name, which the code reads or sets it by, and a type, Number or Vector 2, 3 or 4. **Add Input** and **Add Output** add one; the cross takes one away.
- **Code**: Slang that sets every output from the inputs. For an input `edge` and an output `glow`, `glow = float3(edge, edge * 0.5, 0.0);`.

The shader declares the code once as a function, and calls it once for the same inputs, however many nodes hold the same code. Its code runs on the graphics card only, so its preview, and those of the nodes after it, are striped, and nothing after it is worked out once a material on the CPU. Under it is a guess at what it costs, counted from its sums and calls; a loop counts once. A mistake in its code shows as the shader failing to compile: its Inspector's **State** says what is wrong and on which line of the Slang under **Code**.

## Particle graphs

A particle graph moves and draws a Particle System's particles where its settings leave off: its **Update Output** each step of the simulation, its **Look Output** where each particle is drawn. Its search offers a particle's inputs, Age, Life, Seed, Velocity, Delta Time, Particle Color and Particle Size, in place of a surface's, and its previews show a particle's life lived from left to right across the ball. Its properties are worked into it as the numbers they are set to. A picture property names its picture on its **Picture** row, as no material picks one for it; a particle graph reads three pictures at most, at their full size. How a system picks one and what each output does is in [[Particle Systems]].

## Sub-graphs

A sub-graph is nodes that other VoxShade graphs use as one node: a rim of light, a pattern, a way of blending. Make one with **New Shader… > Sub-graph**. It is a `.voxshade` file, but not a shader: materials do not offer it.

In a sub-graph, **Inputs and Outputs** beside the canvas take the place of Properties. Each input has a name, a type and a **Default**, which it takes where a graph using it leaves it unwired; each output has a name and a type. The search's **Sub-graph** folder has a node for each input, and the **Sub-graph Output** takes a value for each output. A sub-graph has no properties of its own: bring values in through its inputs. Its previews show its inputs at their defaults.

In another graph, the search's **Sub-graphs** folder adds one, or pick which on a Sub-graph node's **Sub-graph** row. Its inputs and outputs are the sub-graph's, and change as it changes. When a graph is compiled, each sub-graph it uses is worked into it, so what the sub-graph works out is folded, shared and left out with everything round it, and costs what the same nodes in the graph would. Its cost shows under the node that uses it. A sub-graph can use others, but not itself. An input can be a **Picture**, the last of its Type's choices: wire a picture property's node into it where the sub-graph is used, and inside, its Input node into a **Sample**. It has no default, so it must be wired. A sub-graph's outputs are numbers.

## Properties

A shader's properties are the values each material using it sets. Add one with **Add Property** under Properties, and give it a **Name**, which the material file keeps it under, and a **Kind**: Number, Vector 2, 3 or 4, Color, Color with Alpha, or Picture. **Default** is what a material starts with. A Number can have a **Range**, the least and most a material may set it to. **Tooltip** says what it does when the pointer rests on it in a material's Inspector.

To read a property in the graph, add it from the search's Properties folder, or pick it on a Property node's **Property** row. A picture is read with its **Sample** node, at the UV wired into it.

## The shader's settings

- **Name**: what the shader's Slang calls its properties, made of letters, digits and _.
- **Unlit**: drawn with its surface's colour and emission alone, without light, as a hologram is.
- **Max Offset**: how far its Vertex Output moves a vertex at most, in metres, so nothing that sways out of its usual bounds is culled while still in view.

## The Slang it makes

Select a `.voxshade` file in the Project panel to see what the Inspector makes of it: its properties, its functions, whether it compiles, and under **Code** the Slang the graph turns into, read only. The values worked out once a material are at the end of its properties struct, named `worked0`, `worked1` and on. To change that Slang by hand, copy it into a `.shader` file of your own; the engine does not fill those values for a `.shader` file, so work them out in the Slang again.
