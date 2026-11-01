---
order: 3
---

A **prefab** is an Obj saved as a file of its own, children and components included, so you can use it again and again. Make a lamp once as a prefab, and every lamp in your game can be a copy of it. Change the prefab, and every copy changes with it.

Each copy is called an **instance**. An instance can still differ from its prefab: one lamp can glow red while the rest glow white.

## Making a prefab

Drag an Obj from the Hierarchy onto the Project panel. Voxear writes it as a `.prefab` file in the folder the Project panel shows, named after the Obj, and the Obj in your level becomes an instance of it.

## Changing a prefab

Double-click the prefab in the Project panel. It opens on its own in the Scene view, with a bar along the top. Change it as you would any Obj, and save with Ctrl+S. Click **Back** to return to your level, where every instance now shows the change.

You can also open a prefab from one of its instances: right-click the instance in the Hierarchy and choose **Open Prefab**.

## Placing more instances

Drag the prefab from the Project panel into the Scene view. The new instance lands where the pointer meets the ground, or on top of whatever is under the pointer. If there is nothing there, it lands a little way in front of the camera.

You can also drag the prefab onto the Hierarchy:

- Let go on an Obj to put the instance inside that Obj.
- Let go near the top or bottom edge of a row to put it just before or after that Obj. A line shows where it will go.
- Let go on a scene's name to put it in that scene.

Press Ctrl+Z to take a placed instance away again.

To copy an instance you already have, select it and press Ctrl+D. Each copy is another instance of the same prefab.

Your scripts can make instances while the game runs, which is how you spawn bullets, enemies or pickups:

```csharp
using System.Numerics;
using Voxear;

namespace MyGame
{
	public sealed class LampMaker : Behaviour
	{
		[SerializeField] private Prefab lamp;

		protected override void Start()
		{
			var ahead = new Vector3(0f, 0f, 5f);
			lamp.Instantiate(ahead, Quaternion.Identity);
		}
	}
}
```

Point the `lamp` field at the prefab in the Inspector. See [[Fields in the Inspector]] for how fields work.

## Instances that differ from their prefab

Change an instance in the Inspector, and the change belongs to that instance alone. Everything you have not changed still follows the prefab.

## Prefabs inside prefabs

A prefab can hold instances of other prefabs: a street prefab can hold several lamp prefabs. Changing the lamp prefab changes the lamps in every street.

A **variant** is a prefab built on another, with a few changes of its own, such as an amber lamp built on the plain lamp. Changes to the plain lamp still reach the amber one. For now, variants are made by writing the prefab file:

```toml
# Voxear prefab: a lamp in amber, built on the plain lamp.
format = 1

[[obj]]
id = 1
name = "Amber lamp"
prefab = "Prefabs/Lamp.prefab"
[obj.light]
type = "point"
color = [1.0, 0.72, 0.3]
intensity = 6.0
range = 5.0
```

## Locking components for designers

In a prefab you have opened on its own, right-click a component's title and choose **Lock for Designers**. A padlock shows on it. Then right-click each value designers should still see and choose **Show to Designers**.

On the prefab's instances, a locked component can no longer be removed, and shows only the values you chose. This lets a programmer fix how a component is set up, and leaves the people placing the prefab only the values meant for them.

> [!NOTE]
> Two changes to an instance are not saved yet: deleting an Obj inside it, and removing a Mesh Renderer, Light or Camera from it. They come back from the prefab the next time the level opens.
