---
order: 8
---

**VoxShade** makes custom shaders out of nodes rather than written in Slang. You wire nodes together on a canvas, and the editor turns them into the same kind of shader a `.shader` file holds. Materials pick a VoxShade shader as their **Shader** and set its properties in the Inspector, exactly as they would for one written by hand (see [[Custom Shaders]]).

A VoxShade shader costs what the same shader written by hand costs. Before any Slang is written, the editor works out everything it can: numbers known when the graph is saved, work that changes nothing (adding nothing, multiplying by one), the same work done by two nodes, and nodes nothing reaches. Hologram and Sway made again as graphs draw the same pictures as the hand-written ones, in the same time on the graphics card.

## Making a VoxShade shader

Right-click in the Project panel, choose **New Shader…**, and pick **VoxShade**. It starts as a tint over a picture, lit as the Standard shader is. **Hologram (VoxShade)** and **Sway (VoxShade)** start as the built-in shaders made of nodes, to change.

Double-click the `.voxshade` file, or press **Open** in its Inspector, to open it in the **VoxShade** panel.

## The VoxShade panel

The graph fills the middle. Down the side are the picked node's values, the shader's properties and its own settings. The bar along the top shows what each stage of the shader costs.

- **Add a node:** right-click on the canvas, press **Space**, or click **Add Node**, then type to search. The nodes are in folders: Inputs, Values, Properties, Math, Logic, Vectors, Noise and Outputs.
- **Wire nodes:** drag from an output on a node's right to an input on another's left. Let go over empty canvas and the search opens, to add a node on the end of the wire. Drag from a wired input to pull its wire off.
- **Pick and move:** click a node to pick it. Shift-click or Ctrl-click to pick more, or drag a box round them. Drag picked nodes to move them.
- **Look round:** the mouse wheel zooms. Drag with the middle button, or with Alt held, to move the view. **F** or **Frame** fits the whole graph in view.
- **Delete**, **Ctrl+D** duplicates, **Ctrl+C** and **Ctrl+V** copy and paste, with the wires between the nodes copied. **Ctrl+Z** and **Ctrl+Y** undo and redo while the panel is the one you last clicked in.

Each change is saved as you make it, and the shader compiles into the engine's own in the background, as a `.shader` does when it is saved. Every material using it changes a few seconds later.

### What a node costs

Under each node is what it costs once the graph is compiled: how many instructions it takes, roughly, and how many pictures it reads. A node with nothing under it costs nothing, because it was worked out when the graph was saved, its work is shared with another node, or it reaches nothing. The bar adds up each stage: **Surface** runs once a pixel, **Vertex** once a vertex and **Light** once a pixel for each light.

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
| Properties | Each of the shader's properties, and **Sample** for each picture property |
| Math | Add, Subtract, Multiply, Divide, Minimum, Maximum, Power, Negate, Absolute, Floor, Ceiling, Fraction, Sine, Cosine, Square Root, Exponential, Saturate, One Minus, Sign, Normalize, Length, Distance, Dot, Cross, Reflect, Step, Lerp, Clamp, Smoothstep |
| Logic | **Compare** gives true or false; **Branch** gives one value or another as a condition holds |
| Vectors | **Split** a vector into its numbers, **Combine** numbers into one, **Swizzle** its numbers into another order |
| Noise | **Random**: a number from 0 to 1 that looks random and is the same for the same seed |
| Outputs | **Surface Output** sets the surface's colour, alpha, normal, metallic, roughness and emission. **Vertex Output** moves each vertex. **Light Output** replaces the standard lighting with your own |

Most Math nodes work on numbers of any width: wire a Vector 3 into an Add and it adds Vector 3s. A single number wired in beside a vector stands for that number in every place.

A **Branch** worked out once a draw, as on a property, only runs the side it takes, when that side is worth skipping. One that changes from pixel to pixel runs both sides unless one reads a picture or does a lot of work, because pixels running together wait for each other either way.

## Properties

A shader's properties are the values each material using it sets. Add one with **Add Property** under Properties, and give it a **Name**, which the material file keeps it under, and a **Kind**: Number, Vector 2, 3 or 4, Color, Color with Alpha, or Picture. **Default** is what a material starts with. A Number can have a **Range**, the least and most a material may set it to. **Tooltip** says what it does when the pointer rests on it in a material's Inspector.

To read a property in the graph, add it from the search's Properties folder, or pick it on a Property node's **Property** row. A picture is read with its **Sample** node, at the UV wired into it.

## The shader's settings

- **Name**: what the shader's Slang calls its properties, made of letters, digits and _.
- **Unlit**: drawn with its surface's colour and emission alone, without light, as a hologram is.
- **Max Offset**: how far its Vertex Output moves a vertex at most, in metres, so nothing that sways out of its usual bounds is culled while still in view.

## The Slang it makes

Select a `.voxshade` file in the Project panel to see what the Inspector makes of it: its properties, its functions, whether it compiles, and under **Code** the Slang the graph turns into, read only. To change that Slang by hand, copy it into a `.shader` file of your own.
