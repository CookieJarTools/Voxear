---
order: 1
---

An **Obj** is one thing in your game's world, such as a crate, a light or the player. (In other engines it is called a game object or an entity.) Every Obj has a name and a **Transform**: where it is, which way it faces, and how big it is.

What an Obj does comes from its **components**. A cube is an Obj with a Mesh Renderer, which draws its shape. A lamp might be an Obj with a Light, plus a script that makes it flicker.

## Making an Obj

Click the **+** at the top of the Hierarchy and pick what to make:

| Choice | What it makes |
| --- | --- |
| Empty | An Obj with nothing but a Transform, to hold other Objs or a script |
| Cube, Sphere, Plane | A shape, drawn by a Mesh Renderer |
| Directional Light | Light from far away in one direction, like the sun |
| Point Light | Light shining out from a point, like a bulb |
| Spot Light | A cone of light, like a torch |
| Camera | What the game is seen through |

The new Obj appears in front of you in the Scene view. The command bar (Ctrl+K) has the same choices, as **New Cube**, **New Point Light** and so on.

Right-click an Obj in the Hierarchy to **Duplicate** it (Ctrl+D), **Delete** it, or make a new Obj inside it.

To rename an Obj, type its new name in the box at the top of the Inspector.

## Objs inside Objs

An Obj can hold other Objs, called its children. Children move, turn and grow with the Obj they are in, so a car's wheels go wherever the car goes.

To put one Obj inside another, drag its row in the Hierarchy onto the other's row. Drop it near the top or bottom edge of a row instead to put it before or after that Obj; a line shows where it will go. The Obj stays where it was in the world.

## An Obj's Transform

The Inspector shows the Transform of the selected Obj first:

- **Position**, in metres, from the Obj it is in, or from the middle of the world.
- **Rotation**, in degrees around each axis.
- **Scale**, where 1 is its own size.

Y is up, and Z is forward. Right-click a value to **Reset** it.

## An Obj's components

Under the Transform, the Inspector shows each component, each with its own settings. The built-in ones are:

- **Mesh Renderer** draws a model with a material, which gives it its colour and surface.
- **Light** shines on everything around it.
- **Camera** is what the Game view, and the player, see through.
- **LOD Group** shows one of the Objs inside it at a time, by how big it is on screen or how far away it is. See [[Levels of Detail]].

Your own scripts are components too. They are called **behaviours**; see [[Your First Script]].

Click **Add Component**, under the last one, to add another: the built-in ones the Obj does not have yet, then your behaviours. Your behaviours appear once your scripts have built.

Right-click a component's title to **Remove Component**, or, for a behaviour, to **Move Up** or **Move Down**. Behaviours on an Obj run in the order they are listed.

> [!NOTE]
> The Inspector also shows **Tag**, **Layer** and **Static** for an Obj, and a switch to turn it on and off. These are not used yet, and are not saved. **Persistent**, below them, does work: it keeps the Obj through changes of level. See [[Scenes and Levels]].
