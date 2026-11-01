---
order: 12
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
| `Label(position, text)` | Text that faces the camera |

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
- `Duration(seconds)` keeps drawings up for that long instead of one frame. Use it for things that happen once, such as where a bullet hit.
- `OnTop()` draws over everything, instead of faintly behind things.
- `InLocalSpace(obj)` draws in an Obj's own space, so a point at `(0, 1, 0)` is one unit above it, turned and scaled with it. `Matrix(matrix)` does the same with any matrix.
- `InGame()` marks drawings as part of your game, so they show in release builds too.

Blocks can sit inside each other, and each one only changes what it says.

## Where drawings show

Drawings show in the editor's Scene and Game views, and in development builds of your game. Release builds leave them out, except what you draw inside `InGame()`.

`Draw` is still called in a release build and costs a quick check each time. If you draw a lot, put it behind a setting of your own. The older `Debug.Line`, `Debug.Sphere` and `Debug.Text` calls do the same as `Draw`'s, but are removed from release builds entirely.

Next: [[Your First Menu]], to build a game's own UI.
