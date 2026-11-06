---
order: 14
---

`Draw` puts lines, shapes and labels in the 3D view, so you can see what your code is doing: where an enemy is looking, how far a sound carries, the path a character means to take. You can call it from anywhere, at any time.

```csharp
Draw.WireSphere(Obj.WorldPosition, hearingRange);
Draw.Arrow(Obj.WorldPosition, Obj.WorldPosition + velocity);
Draw.Label(Obj.WorldPosition, $"{health} HP");
```

A drawing lasts one frame, so code that draws every frame always shows what is true now. Lines behind other things are drawn faintly, so you can still see them without them hiding what is in front.

## The shapes

| Call | What it draws |
| --- | --- |
| `Line(from, to)`, `Ray(origin, direction)`, `Arrow(from, to)` | A line, a line along a direction, a line with a head |
| `Lines(pairs)`, `Polyline(points, closed)` | Many lines at once, or one line through many points |
| `Cross(point, size)` | A small mark at a point |
| `Circle(center, normal, radius)`, `Arc(center, normal, from, degrees, radius)` | A circle, or part of one |
| `WireBox(center, size, rotation)`, `WireSphere(center, radius)` | A box's edges, a sphere's outline |
| `WireCapsule(start, end, radius)`, `WireCylinder(start, end, radius)` | A capsule or a cylinder between two points |
| `WireCone(apex, direction, degrees)` | A cone, as a spot light's |
| `Frustum(position, rotation, fov, aspect, near, far)` | What a camera sees |
| `Bezier(start, startHandle, endHandle, end)` | A smooth curve, as a path or a cable |
| `Grid(center, size, cells)` | A grid on the ground |
| `SolidBox(center, size, rotation)`, `SolidSphere(center, radius)` | A solid box or ball, its sides shaded apart |
| `Triangle(a, b, c)` | A solid triangle, seen from both sides |
| `Label(position, text)` | Text that faces the camera |
| `ScreenText(text)` | A line of text in the view's top-right corner, under the ones before it: a value to watch, as `Draw.ScreenText($"speed {speed:0.0}")` |

Every shape takes a colour last, if you want one: `Draw.Line(a, b, Color.Red)`.

## Setting how things are drawn

A `using` block changes how everything inside it is drawn, and puts things back when it ends:

```csharp
using (Draw.Color(Color.Yellow))
using (Draw.Width(3f))
{
	Draw.WireSphere(target, 0.5f);
	Draw.Line(Obj.WorldPosition, target);
}
```

- `Color(color)` draws in that colour.
- `Width(pixels)` draws thicker lines.
- `WidthInMetres(metres)` draws lines as wide as something that size in the world, narrower the farther away they are: a rope, a laser, a path on the ground.
- `Duration(seconds)` keeps drawings up for that long instead of one frame. Use it for things that happen once, such as where a bullet hit.
- `OnTop()` draws over everything, instead of faintly behind things.
- `InLocalSpace(obj)` draws in an Obj's own space, so a point at `(0, 1, 0)` is one unit above it, turned and scaled with it. `Matrix(matrix)` does the same with any matrix.
- `InGame()` marks drawings as part of your game, so they show in release builds too.

Blocks can sit inside each other, and each one only changes what it says.

## Gizmos

A behaviour can draw **gizmos**: drawings that show where things are while you build the level, such as how far an enemy can hear or where a door swings. Override `DrawGizmos`:

```csharp
public sealed class Guard : Behaviour
{
	public float HearingRange = 8f;

	protected override void DrawGizmos(Gizmo gizmo)
	{
		var color = gizmo.IsSelected ? Color.Yellow : Color.Grey;
		Draw.WireSphere(Obj.WorldPosition, HearingRange, color);
	}
}
```

The editor calls it every frame, for every Obj with the behaviour, while you edit and while the game plays. While you edit, the behaviour is made from the values in its scene, so changing a value in the Inspector changes the gizmo straight away. `gizmo.IsSelected` says whether its Obj is selected, and `gizmo.IsHovered` whether the pointer is over it, so a gizmo can show more when it matters.

Gizmos show in the Scene view while its **Gizmos** button is on. They never show in the Game view or in your built game. Any shape and scope from `Draw` works in a gizmo.

Clicking a gizmo's line selects the Obj that drew it, so an Obj you cannot see, such as a trigger or a spawn point, can still be picked out in the Scene view.

### Gizmo icons

An Obj with nothing to see can show an icon where it is, as lights and cameras do. Give the behaviour `[GizmoIcon]`:

```csharp
[GizmoIcon(IconName.Audio)]
public sealed class Ambience : Behaviour
{
}
```

Every Obj with the behaviour shows the icon in the Scene view, and a click on the icon selects the Obj. `IconName` lists Voxear's icons, so a mistyped name will not compile.

### The Gizmos menu

The arrow beside the **Gizmos** button opens a menu of which gizmos show: lights, cameras, and every behaviour type that draws gizmos or has an icon. Untick one to hide it in every Scene view. A hidden gizmo cannot be clicked either, and the editor remembers the choice.

To drag a value in the Scene view instead of drawing it, a handle attribute on the field is often enough: see "Editing fields in the Scene view" in [[Fields in the Inspector]]. For anything those do not cover, use `Handles`.

## Handles

`Handles` puts things to drag in the Scene view, from inside `DrawGizmos`. Each takes a value as it is now and gives it back as the drag has left it, so all your code does is assign it back:

```csharp
public sealed class Patrol : Behaviour
{
	public List<Vector3> Points = [new(0f, 0f, 0f), new(3f, 0f, 0f)];
	public float WaitRange = 1f;

	protected override void DrawGizmos(Gizmo gizmo)
	{
		for (var i = 1; i < Points.Count; i++)
		{
			Draw.Line(Points[i - 1], Points[i], Color.Cyan);
		}
		if (!gizmo.IsSelected)
		{
			return;
		}
		for (var i = 0; i < Points.Count; i++)
		{
			Points[i] = Handles.Position(Points[i]);
		}
		WaitRange = Handles.Radius(Points[0], WaitRange);
		if (Handles.Button(Points[^1] + new Vector3(1f, 0f, 0f), Color.Green))
		{
			Points.Add(Points[^1] + new Vector3(2f, 0f, 0f));
		}
	}
}
```

| Handle | What you drag | What it gives back |
| --- | --- | --- |
| `Handles.Position(point)` | A dot, across the view | The point it was dragged to |
| `Handles.Position(point, arrows: true)` | The dot, or a red, green or blue arrow to move it along one axis | The point it was dragged to |
| `Handles.Radius(center, radius)` | A dot on the edge of a sphere, which it draws | The new radius |
| `Handles.Slider(origin, direction, distance)` | A dot along a line, which it draws | How far along the line it is |
| `Handles.Button(point)` | A square you click | `true` in the frame it was clicked |
| `Handles.Disc(center, rotation, axis, radius)` | A dot round a ring about the axis, which it draws | The rotation, turned as far round the axis |

Points are in the world. Each handle also takes a colour. Holding Ctrl while you drag snaps a point to the grid's snap distance, a radius or distance to tenths of a metre, and a disc to 15° turns.

While you edit, a drag changes the behaviour's values in the scene, just as typing them into the Inspector would: the whole drag is one step to undo, and a field's `[OnValueChanged]` method runs when a handle changes it. So does a click on a button, so adding a point with the button above can be undone too. While the game plays, a drag changes the running behaviour, and stopping the game puts it back as it was.

Handles are told apart by the order `DrawGizmos` shows them in. Show the same handles in the same order each frame, or a drag may jump from one to another.

## Where drawings show

Drawings show in the editor's Scene and Game views, and in development builds of your game. Release builds leave them out, except what you draw inside `InGame()`.

`Draw` is still called in a release build and costs a quick check each time. If you draw a lot, put it behind a setting of your own. The older `Debug.Line`, `Debug.Sphere` and `Debug.Text` calls do the same as `Draw`'s, but are removed from release builds entirely.

Next: [[Your First Menu]], to build a game's own UI.
