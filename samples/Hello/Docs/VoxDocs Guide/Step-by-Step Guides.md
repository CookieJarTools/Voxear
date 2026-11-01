---
id: 3e9c51b0d7a4
tags: [docs, guide]
updated: 2026-11-01
---

A guide can do the work for the reader. A link to an action or a check, on a line of its own, is a step: an action is a button that runs a command, and a check ticks itself once what it looks for is there. The line at the top says how far you have got.

## A guide to try

1. Swap the two spinners, with the command Hello's `Tools.cs` adds.

   [Swap the spinners](<action:Swap Spinners>)

   A check written in C#, in the same `Tools.cs`, ticks once they have swapped:

   [Spinner A stands on the right](check:code/Hello.Tools.SpinnerAOnTheRight)

2. Add a point light to the level.

   [Add a point light](<action:New Point Light>)

   This step ticks itself as soon as the light is there:

   [There is a Point Light](<check:obj/Point Light>)

3. Checks look at the project too. Hello comes with a lamp:

   [The lamp prefab is there](check:file/Prefabs/Lamp.prefab)

> [!TIP]
> What you have done stays ticked on your machine, through restarts, and nobody else's. **Reset** starts the guide again. Undo takes back what an action did, as for anything else.

## Writing steps

- `[words](action:Title)` runs the command with that title, as the command bar lists it: the editor's own, such as `Play` or `New Cube`, or one a script adds with `[Command]`.
- `[words](check:obj/Name)` ticks while there is an Obj with that name.
- `[words](check:component/Light)` ticks while some Obj has that component, or a behaviour of that name.
- `[words](check:file/Prefabs/Lamp.prefab)` ticks while the project has that file.
- `[words](check:code/Hello.Tools.SpinnerAOnTheRight)` ticks while a static method of the scripts, marked `[DocsCheck("…")]` and returning `bool` or `string`, passes: true, or nothing to say. A string says what is still to do, under the step. It looks at the level being edited, or the game while playing.
- `[words](action:create/Material)` makes a material beside the project's others, as the Project panel's New menu does, and `[words](<action:create/Material?at=Materials/Brass.material>)` makes it just there. A Folder, Scene, Level or Script can be made the same way.

While writing, **Action…** under the title puts any of these in at the cursor.

Put a title with spaces in angle brackets, as `[Add a light](<action:New Point Light>)`.
