---
order: 8
---

A **Route** is a way through a world that travellers keep to: a cart track, a shipping lane, a flight path. Routes of a kind that meet are joined, and a script asks for the way from one place to another along them. Every [[Roads|Road]] is a route of kind road, so the roads you lay are found too.

A **Route Stop** is a place on a route: a harbour, an inn, a waystation. It holds a data asset of your game's own, and a way found along routes lists the stops it passes.

## Making a route

1. Make an empty Obj and choose **Add Component ▸ Terrain ▸ Route**.
2. Drag its points in the Scene view, as a road's. The faint dots add points, Shift-click takes one away, and the small dot at the side sets how wide it is.
3. Pick its **Kind**: **Road**, **Sea**, **Air**, or **Custom** with a **Route Kind** of your own (choose **New Data Asset… ▸ World ▸ Route Kind**).

A route's points within a few metres of another route of its kind, or its end near another's line, join them. A stop joins the nearest route within 24 metres of it.

The Scene view draws each route in its kind's colour. Turn them off under **Gizmos ▸ Areas and Routes**. The game's maps draw them too, unless **On Map** is off.

## Settings

| Setting | What it does |
| --- | --- |
| Kind, Route Kind | What travels it |
| Width | How wide it is, in metres |
| One Way | It is travelled only from its first point towards its last |
| Draught | A sea route's least depth of water, in metres |
| Shore | How far a sea route keeps from land, in metres |
| On Map | Whether the game's maps draw it |

## Sea routes

A sea route lays itself. Put its first point at one harbour and its last at the other, and lay it: it finds a way over water at least **Draught** deep and at least **Shore** from land, searched over the world's generated ground, however far off and whether or not its blocks are made. Its ends are joined straight to the nearest such water.

A laid route is saved as its points. If you paint land under it later, it is drawn red in the Scene view where it runs aground. Lay it again to go round.

```
vox editor call route.lay_sea obj=Lane
```

## Finding the way

```csharp
if (Routes.Find(Obj.Position, market.Position, RouteKind.Road) is { } way)
{
	Log.Info($"{way.Length:0} m, by {way.Stops.Count} stops");
}
```

A traveller follows a way by how far along it they are:

```csharp
travelled += speed * Time.Delta;
var (at, heading) = way.At(travelled);
Obj.Position = at;
```

`Routes.Nearest` gives the nearest place on a route of a kind and how far along the route it lies. `Routes.LaySea` gives a sea way's points between two places. `Obj.Route` and `Obj.AddRoute` give every setting above, and `Route.LayOverSea` lays a sea route between its ends. `Obj.RouteStop` gives a stop's **Data**.

From the command line:

```
vox editor call route.set_points obj=Track points='[[0,0,0],[80,0,20],[140,0,90]]' land=true
vox editor call routes.find from=[0,0,0] to=[140,0,90] kind=road
vox editor call routes.nearest at=[60,0,40] kind=sea
```
