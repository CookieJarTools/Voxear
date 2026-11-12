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
| **Size** | How wide the minimap is, in pixels on a screen 1080 high |
| **Zoom** | How many blocks one cell of the minimap is, from 1:1 to 1:64 |
| **Turn With View** | On, the minimap turns so what is ahead is at its top. Off, north stays at its top |
| **Round** | Cuts the minimap's ground round rather than square |
| **Tilt** | How far over the ground the minimap is seen from, in degrees. 90 looks straight down |
| **Open Map** | The input action that opens and closes the map. With none, only a script opens it |
| **Map Zoom** | How many blocks one cell of the map is when it opens |
| **Map Size** | How wide the map's ground is drawn, as a percentage of the screen's height |
| **Map Tilt** | How far over the ground the map is seen from when it opens, in degrees |
| **Pointer Control** | Whether the pointer turns, moves and zooms the map. Off, only scripts do |
| **Shows** | What the maps show of the world: **Everything**, what was **Explored**, or what the **Game Decides** (see [What a map knows](#what-a-map-knows)) |

Each step of zoom doubles the ground a map covers and halves its detail, so a map costs the same at any zoom. An opened map covers 110 to 220 cells, more as it is zoomed out: 440 to 880 m at 1:4 with metre blocks.

Every size here is for a screen 1080 pixels high. On another, the minimap and the map are scaled with the screen's height, so they take up as much of a small window as of a large screen.

## What the minimap and the map show

The **minimap** is a panel with a window on the ground. The ground fills the window to every corner, at any zoom and however the map is turned. In the window's corners are a small compass and its zoom. Under the window, a strip names the country, with the time beside it, and under that says where the player is and how warm and wet it is there.

The **map** covers the screen, and keeps its middle and its bottom left clear:

- Top left, the country's name over where the map looks, along x and z.
- Top right, the time, the temperature and the humidity.
- Bottom right, a compass lying on the ground as the map does, its needle pointing north, over buttons that zoom in and out a step.
- Over the player, an arrow lying on the ground, pointing the way they face, and their name on a tag over it. The arrow lies on the ground under the player, though they fly over it or walk under it.

What stands on the ground stands on the map, as small as the map's zoom makes it:

- **Structures** the world's generator builds (see [[Structures]]): houses, a town's streets and roads, a dungeon's way in, and woods of block plants. They are their own blocks at 1:1. Zoomed out, a house is its footprint with its roof over its walls, and a road is the ground's own top, out to 1:8. In larger cells a house is too small a part of a cell to show.
- **Plants the world grows** (see [[Block Worlds#Plants a world grows]]) that have a **Far block**: each stands where it grows, drawn in that block, while it is at least a cell across. Zoomed further out they show as woods, as they do in the far view.
- **What was built and dug**, block for block at 1:1.

Small plants with no Far block (grass, flowers, bushes, rocks) are not on the map, nor are meshes placed in the scene: give those a Map Marker. Under the ground only the first few blocks show, so a dungeon is its way in and not its rooms.

The country's name is how warm and wet the world's generator makes that place, as Forest or Tundra. The time is whatever your game says it is, with `Map.Time`, and shows only once it says. The player's name is `Map.PlayerName`, or the label of their Map Marker.

## The opened map

Pick **Open Map** from the project's input actions (see [[Reading Input]]). Pressing it opens the map over the game, and pressing it again closes it. While the map is open the mouse is free, and only the input map that action is in stays on, so the game hears nothing meant for the map. Put the action in an input map of its own, or in the one your menus use. A script opens and closes it too, with `Open()` and `Close()`, whether or not it has an action.

| To | Do |
| --- | --- |
| Turn and tilt the map | Drag |
| Move over the ground | Drag with the middle button |
| Zoom | Turn the wheel |

Zooming keeps the map the size it is on the screen and shows more or less ground. The player's arrow stays at the ground's edge, towards where they are, once the map is moved off them. With **Pointer Control** off the pointer does none of this, and a script steers the map (see [Steering a map](#steering-a-map)).

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

What the maps follow has a mark of its own, the accent arrow, so its Map Marker is not drawn as well.

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

`Map.AddMarker` with an Obj gives it a Map Marker. With a place it gives back a `MapPin`, which you move, restyle or `Remove()`.

| On every map | What it sets |
| --- | --- |
| `Map.Follow` | The Obj the maps follow, in place of the camera |
| `Map.Title` | The country's name, in place of the generator's |
| `Map.Time` | The time shown, as you write it: `Map.Time = $"{hour:00}:{minute:00}";` |
| `Map.PlayerName` | The name over the player's arrow on the opened map |

`Obj.Map` is an Obj's Map, with the settings above and `Open()`, `Close()` and `IsOpen`. `Obj.AddMap()` gives an Obj one. Hold it in a variable to change its settings:

```csharp
var map = Obj.Map;
map.MapSize = 1.3f;
map.PointerControl = false;
map.Open();
```

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

From code they are `Minimap` and `MapView`:

```csharp
var minimap = hud.Root.Add(new Minimap { Zoom = 4, Round = true });
var map = hud.Root.Add(new MapView { Zoom = 8, Fill = 1.3f });
map.Visible = false;
```

Both have every setting below, by the same names in markup as attributes. Each can be changed while the map shows.

| Setting | In markup | What it does |
| --- | --- | --- |
| `Zoom` | `zoom` | How many blocks a cell is to begin with, 1 to 64 |
| `TurnsWithView` | `turns` | Turns with what the maps follow, or keeps north at its top |
| `Round` | `round` | Cuts the ground round rather than square |
| `Tilt` | `tilt` | How far over the ground it is seen from to begin with, in degrees |
| `Fill` | `fill` | How wide the ground is drawn, as a share of the view's height |
| `CoversView` | `cover` | Whether the ground is drawn larger than that where it takes it for none of its edges to show. On for a minimap, off for a map |
| `PointerControl` | `controls` | Whether the pointer turns, moves and zooms it |
| `MinZoom`, `MaxZoom` | `min-zoom`, `max-zoom` | The fewest and the most blocks a cell may be as it zooms |
| `Shadows` | `shadows` | How dark its shadows are, 0 to 1 |
| `EdgeLines` | `edges` | How dark the lines along its blocks' edges are, 0 to 1 |
| `Thickness` | `thick` | How thick the ground is under its lowest place, in cells |
| `ShowArrow` | `arrow` | Whether what the maps follow is marked |
| `ArrowStaysAtEdge` | `arrow-at-edge` | Whether that mark stays at the ground's edge once the map is moved off it |
| `ShowLabels` | `labels` | Whether markers' labels show |
| `ShowTitle` | `title` | Whether the country's name shows, with a map's panel round it |
| `ShowReadout` | `readout` | Whether the time, temperature and humidity show: a map's strip, and all of a minimap's |
| `ShowCompass`, `ShowZoom` | `compass`, `zoom-control` | Whether the compass shows, and the zoom: a map's buttons, a minimap's tag |
| `ReferenceHeight` | `reference` | The screen height its sizes are for, 1080 to begin with. 0 leaves them unscaled |

A Map component's own minimap and map are the same two elements: `Obj.Map.MinimapView` and `Obj.Map.MapView` give them, once they are on the screen, so every setting here is theirs too. The component's own settings win where both say.

A `Minimap` is 380 pixels wide on a screen 1080 high, unless its style gives it another `width`. A `MapView` covers what it is in. Showing and hiding a `MapView`, and what the game does with its input meanwhile, are yours to decide.

Style sheets reach every part by its class (see [[UI Style Sheets]]). A part's sizes are its own style, so that they scale with the screen: a sheet changes its colours and its type.

| Class | What it is |
| --- | --- |
| `minimap`, `map` | The whole of a minimap, or of a map |
| `map-panel` | Every panel: the fill and the border |
| `map-picture`, `map-overlay` | The ground, and what lies over it |
| `map-label` | A marker's label |
| `map-region`, `map-title` | A map's panel for where it looks, and the country's name |
| `map-environment`, `map-cell` | A map's strip for the time and the air, and each of its three cells |
| `map-caption`, `map-number`, `map-unit`, `map-axis` | The small names, the numbers, their units, and the X and Z |
| `map-compass`, `minimap-compass`, `map-compass-north`, `map-compass-letter` | The compasses, the N, and the other letters |
| `map-zoom`, `map-zoom-button`, `map-zoom-value`, `minimap-zoom` | A map's zoom panel and its parts, and a minimap's zoom tag |
| `map-tag`, `map-avatar`, `map-tag-name` | The player's name tag, its tile and the name |
| `minimap-window`, `minimap-info`, `minimap-row` | A minimap's window, its strip and each row of it |

## Steering a map

Each `Minimap` and `MapView` has a `Camera`: how the map is seen, to read or to change.

```csharp
var camera = Obj.Map.MapView!.Camera;
camera.LookAt(town.WorldPosition);
camera.Zoom = 8f;
camera.Turn = 45f;
```

| Member | What it is |
| --- | --- |
| `Turn` | Degrees from north: at 0 north is at its top, at 90 east is |
| `Tilt` | Degrees over the ground, 20 to 90 |
| `Zoom` | Blocks to a cell: any number within `MinZoom` and `MaxZoom` |
| `Offset` | How far its middle is from what the maps follow, in metres east and north |
| `LookAt(place)` | Moves its middle to a place in the scene |
| `Reset()` | North up, over what the maps follow, at its settings' zoom and tilt |
| `PointOf(place)` | Where a place in the scene shows in the view, in the UI's pixels |
| `Shows(place)` | Whether a place is on the ground the map shows now |
| `PlaceAt(point)` | The place in the scene under a point of the view |

`PlaceAt` is how a player marks the map: give it where they clicked, and put a pin there.

```csharp
if (camera.PlaceAt(Mouse.Position) is { } place)
{
	Map.AddMarker(place, MapIcon.Pin, "Go here");
}
```

What is set is done the next time the map is drawn, and what is read is the map as it was last drawn. The camera answers once its map is on the screen with ground to show.

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
- Plants are drawn in one block each, their Far block, so a tree is its leaves' colour from trunk to top.
- The country's name is of the place at the map's middle. `Camera.PlaceAt` gives the place under the pointer, for a name of your own.
- The words on the panels (TIME, TEMP, HUMIDITY, ZOOM) are not translated yet.
- Names and numbers are drawn in the UI's own two typefaces, and labels cannot be letter-spaced.
- A gamepad opens and closes the map through its action. Turning and moving it with one is a few lines on its `Camera`.
- Ground known is drawn as it is now. Only news is kept as it was heard.
