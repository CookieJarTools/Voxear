---
order: 8
---

A **Map** shows the scene's Block World as a small model of itself: hills, cliffs, water and whatever stands on the ground, cut out round the player and seen from above. It comes in two sizes: a **minimap** in a corner of the screen, and a **map** opened over the game that the player can turn, tilt, move and zoom. Both are drawn from the same ground the world is made from, with what was built and dug, and cost nothing while they are left alone.

Add one with the Hierarchy's **+ ▸ Map**, or to any Obj with **Add Component ▸ UI ▸ Map**. With a Block World in the scene (see [[Block Worlds]]), the minimap shows as soon as the game plays.

## Map settings

| Setting | What it does |
| --- | --- |
| **Minimap** | Whether the minimap shows |
| **Corner** | The corner of the screen the minimap sits in |
| **Size** | How large the minimap is, in the UI's pixels |
| **Zoom** | How many blocks one cell of the minimap is, from 1:1 to 1:64 |
| **Turn With View** | On, the minimap turns so what is ahead is at its top. Off, north stays at its top |
| **Round** | Cuts the minimap's ground round rather than square |
| **Tilt** | How far over the ground the minimap is seen from, in degrees. 90 looks straight down |
| **Open Map** | The input action that opens and closes the map |
| **Map Zoom** | How many blocks one cell of the map is when it opens |
| **Shows** | What the maps show of the world: **Everything**, what was **Explored**, or what the **Game Decides** (see [What a map knows](#what-a-map-knows)) |

Each step of zoom doubles the ground a map covers and halves its detail, so a map costs the same at any zoom. A minimap covers about 110 cells across: 220 m at 1:2 with metre blocks. An opened map covers twice that.

## The opened map

Pick **Open Map** from the project's input actions (see [[Reading Input]]). Pressing it opens the map over the game, and pressing it again closes it. While the map is open the mouse is free, and only the input map that action is in stays on, so the game hears nothing meant for the map. Put the action in an input map of its own, or in the one your menus use.

| To | Do |
| --- | --- |
| Turn and tilt the map | Drag |
| Move over the ground | Drag with the middle button |
| Zoom | Turn the wheel |

The map's title names the country it looks at, by how warm and wet the generator makes it there. The line over the title reads where that is, how warm and how wet, and the zoom. The compass lies on the ground as the map does, its red arm pointing north.

## Markers

A **Map Marker** marks its Obj on every map, where it is. Add one with **Add Component ▸ UI ▸ Map Marker**.

| Setting | What it does |
| --- | --- |
| **Icon** | What it is drawn as: a pin, a player, a town, a dungeon, a camp, a note, a chest, a skull, a star or an arrow |
| **Label** | What is written beside it on the opened map |
| **Colour** | Its colour |
| **Shows On** | The minimap, the opened map, or both |
| **Stay At Edge** | Keeps it at a map's edge, towards where it is, once it is past it |

Put a Map Marker on your player's prefab and every player shows on the others' maps, as long as that player's Obj reaches them. In a small co-op game, give the player's Net Object a range of 0 so it reaches everyone (see [[Sharing Objs]]).

What the maps follow is marked with an arrow, pointing the way it faces.

## Maps from scripts

Everything above can be set from a script. What every map follows, its title and its markers are set on `Map` itself, with no Obj to find first:

```csharp
public sealed class Guide : Behaviour
{
	[SerializeField] private Obj town;
	private MapPin? rumour;

	protected override void Start()
	{
		// Every map follows the player rather than the camera.
		Map.Follow = Obj;
		Map.AddMarker(town, MapIcon.Town, "Oakrest");
		// A marker at a place that is no Obj.
		rumour = Map.AddMarker(new Vector3(120f, 30f, -40f), MapIcon.Skull, "Seen here", Color.Red);
	}

	public void HeardAgain(Vector3 place)
	{
		rumour!.Position = place;
	}
}
```

`Map.AddMarker` with an Obj gives it a Map Marker. With a place it gives back a `MapPin`, which you move, restyle or `Remove()`. `Map.Title` sets the opened map's title in place of the country's name.

`Obj.Map` is an Obj's Map, with the settings above and `Open()`, `Close()` and `IsOpen`. `Obj.AddMap()` gives an Obj one.

## A minimap or a map in your own UI

Leave the Map component out, and put the maps in your own HUD instead. In markup they are two elements (see [[UI Markup]]):

```xml
<vui>
	<Panel class="hud">
		<Minimap name="minimap" zoom="4" turns="true" round="true" tilt="60" />
	</Panel>
	<Map name="map" zoom="8" />
</vui>
```

From code they are `Minimap` and `MapView`, with `Zoom`, `TurnsWithView`, `Round` and `Tilt`:

```csharp
var minimap = hud.Root.Add(new Minimap { Zoom = 4, Round = true });
var map = hud.Root.Add(new MapView { Zoom = 8 });
map.Visible = false;
```

A `Minimap` takes its size from its style, 220 pixels square unless a sheet says otherwise. A `MapView` covers what it is in. Showing and hiding a `MapView`, and what the game does with its input meanwhile, are yours to decide.

Style sheets reach every part by its class (see [[UI Style Sheets]]):

| Class | What it is |
| --- | --- |
| `minimap`, `map` | The whole of a minimap, or of a map |
| `map-picture` | The ground |
| `map-overlay` | What lies over the ground: markers and labels |
| `map-label` | A marker's label |
| `map-header`, `map-readout`, `map-title` | The opened map's readout and title |
| `map-compass`, `map-compass-north` | The compass, and its N |

## What a map knows

A map need not show the world as it is. **Shows** on the Map decides:

- **Everything**: all the ground, known or not.
- **Explored**: ground is seen as the player comes near it, and stays seen.
- **Game Decides**: only what your scripts reveal.

Ground a map does not know is drawn as blank paper, flat, whatever stands there.

What is known is kept in a `MapKnowledge`, apart from the world: the ground seen, and **news**, which is what was last heard of each place and when. Nothing in it changes unless a script changes it. So a map can show a dungeon as it was when the player last read of it, though it has grown since.

News is kept in layers you make, each a value from 0 to 1 for a place, coloured from one colour to another:

```csharp
public sealed class NoticeBoard : Behaviour
{
	private static readonly MapLayer Threat = MapLayer.Create(Color.Clear, new Color(0.9f, 0.1f, 0.1f, 0.7f));

	// The player reads the board: their map learns how things stand today.
	public void Read(Vector3 dungeon, float danger, uint today)
	{
		var known = MapKnowledge.Shown;
		known.Reveal(dungeon, 150f);
		known.Paint(Threat, dungeon, 150f, danger, today);
	}
}
```

`MapKnowledge.Shown` is the knowledge the maps draw. `Knows(place)` says whether ground is seen, `News(layer, place)` what was last heard there, and `NewsHeard(layer, place)` when, as the time you gave `Paint`: days played, or seconds, anything that only grows.

### Saving a map

`ToBytes()` gives a map's knowledge as a few bytes, and `Load(bytes)` takes them back. A walk of four kilometres with news of a dozen places is under 3 KB. Keep them in a save beside the rest (see [[Saving the Game]]):

```csharp
file.Set("map", MapKnowledge.Shown.ToBytes());
// Later, loading:
MapKnowledge.Shown.Load(file.Get<byte[]>("map"));
```

### A map for each player

Each player's game shows its own `MapKnowledge.Shown`. A host that decides what players learn keeps one for each of them, and sends each only what is new:

```csharp
// On the host: this player's map, and where it stood when they were last sent it.
var before = theirs.Stamp;
theirs.Paint(Threat, dungeon, 150f, danger, today);
SendNewsOnClient(player, theirs.ChangesSince(before));

// On the player's machine, in the RPC.
MapKnowledge.Shown.Merge(news);
```

`Merge` joins two maps: ground either has seen is seen, and of two pieces of news of a place, the one heard later is kept. Whether a board read by one player updates the whole group is then your choice: merge the news into each player's map, or into one. `ChangesSince` fits in one RPC's bytes for anything a board would say (see [[RPCs]]).

`MapKnowledge.Paper` sets the colour of ground not known, and `MapKnowledge.ExploreRadius` how far round the player ground is seen when the maps show what was explored.

## What maps do not do yet

- They show Block Worlds only, not heightmap Terrain.
- A world's trees and buildings show once its far view carries them: a map is made from the same ground the far view is.
- The title names the place at the map's middle, not the place under the pointer.
- A gamepad opens and closes the map through its action, but does not turn or move it.
- Ground known is drawn as it is now. Only news is kept as it was heard.
