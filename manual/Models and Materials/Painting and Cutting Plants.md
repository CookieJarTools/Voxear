---
order: 5
---

A Foliage's plants can be painted onto the ground, given trunks that collide, and cut down by scripts, a tree at a time. See [[Foliage]] for how plants are drawn.

## Painting plants

Select an Obj with a **Foliage** and pick **Paint** along the top of the Foliage in the Inspector. A ring on the ground under the pointer shows the brush; hold the left mouse button to paint the Foliage's plant there, and hold **Shift** to take plants away instead. Each stroke is one step to undo.

Each plant painted is dropped onto whatever is below it, a terrain or any mesh but other plants, and stands there. The brush paints until as many plants stand under it as the **Density** says, so painting over the same place again does not crowd it.

| Setting | What it does |
| --- | --- |
| Size | The brush's radius, in metres. The terrain brush's, shared: **[** and **]** change it |
| Strength | How fast it paints, 0 to 1. With **Shift**, **[** and **]** change it |
| Density | How many plants a hundred square metres hold where the brush has painted fully |
| Sizes | The smallest and largest plant, each painted somewhere between |
| Lean with ground | How far each plant leans with the slope it stands on, from 0, upright as a tree grows, to 1, square to the ground as a rock lies |
| Steepest ground | The steepest slope plants are painted on, in degrees |
| Spacing | How near two plants may stand, in metres |

These are written with the Foliage. A plant painted is placed one by one, in the scene file's `plants`.

## Trunks

Under **Settings**, turn on **Trunk collider** for plants that things bump into: trees, posts, tall rocks. Each plant gets an upright capsule of the **Trunk radius**, **Trunk height** metres tall from its foot, sized with the plant and leaning with it. Characters walk round them, what falls lands on them, and rays meet them. Grass and bushes need none.

A trunk is a plant's own: a ray that meets one says which plant it was, and a plant taken away takes its trunk with it. Trunks are made on a server too, with no renderer.

## Cutting plants down

A plant taken away while the game runs is no longer drawn, and its trunk goes; the other plants are not touched, so taking a thousand away one by one costs no more than one. Pick a **Cut prefab** under **Settings** for what a plant becomes when it is cut down: a trunk that falls, with a Rigidbody, and a stump, say. It is made where the plant stood, turned as it was and as large.

In a script, a ray's **Plant** is the plant whose trunk it met:

```csharp
if (Physics.Raycast(eye, forward, out var hit, 3f) && hit.Plant is { } tree)
{
    tree.CutDown();
}
```

| In C# | What it does |
| --- | --- |
| `hit.Plant` | The plant whose trunk a ray met, or null |
| `plant.CutDown()` | Takes the plant away and makes its Foliage's Cut prefab there; returns the prefab's root |
| `plant.Remove()` | Takes the plant away, leaving nothing |
| `plant.Exists`, `plant.Position`, `plant.Size` | Whether it stands, where its foot is in the world, and how large it is |
| `Plants.Query(centre, radius, list)` | The plants standing near a point, nearest first: what a blast flattens, or what a woodcutter looks for |

Plants are named by their Foliage's Obj and their number in it, which stays theirs while the game runs. A save of the scene (`Save.WriteScene`) keeps which plants were taken away, and loading it takes them away again.
