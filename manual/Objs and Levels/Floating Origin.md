---
order: 4
---

Positions in a game are kept as floats, which are exact to a step that grows the further a position is from the middle of the world:

| From the middle | Step |
| --- | --- |
| 1 km | a twentieth of a millimetre |
| 8 km | half a millimetre |
| 16 km | a millimetre |
| 100 km | 8 mm |
| 1000 km | 6 cm |

Past a few kilometres, things that move begin to shake, and physics loses its footing. So in a world that goes on without end, the engine now and then moves everything back towards the camera by whole metres, keeping the camera near the middle. This is the **floating origin**: the middle of the world floats along with the camera, and the player never sees it move.

## When the middle moves

Once the camera is more than 2 km from the middle across the ground, everything moves back by whole kilometres, between two frames. A level of a set size never moves: only levels with an [[Endless Worlds|endless terrain]], an endless [[Block Worlds|Block World]] or a [[Voxel Terrain#Planets|planet]] do.

The project's `voxear.toml` can change that:

```toml
[floating_origin]
mode = "automatic"   # or "always", or "never"
distance = 2048      # metres from the middle before it moves
step = 1024          # metres it moves by, a whole number of them
```

| Setting | What it does |
| --- | --- |
| Mode | **Automatic** moves the middle only in levels with ground without end or a planet; **Always** in every level; **Never** keeps it where the level put it |
| Distance | How far the camera goes from the middle, across the ground, before it moves |
| Step | How far it moves at a time. The default, 1024 m, is a whole number of terrain tiles and block chunks, so ground lines up the same after a move |

The middle moves across the ground only, never up or down, and never in a networked game: each player's machine would have a middle of its own, and the server keeps one for everyone.

## What moves with the middle

Everything moves together, so nothing the player sees or hears changes:

- every Obj, by its Transform;
- physics: every body keeps how fast it goes, a sleeping one stays asleep, and characters keep their footing;
- sound: no voice moves or bends in pitch;
- smoke, mist, debug drawing and what the renderer remembers from the frame before, so there is no blur and nothing flickers;
- ground made round the camera, which carries on from where it was without making anything again.

A prop left far behind comes back exactly where it was put, however far the camera goes and returns. Loading a level starts again from its own middle, and Objs that stay through it, such as the player, move with the world.

## Positions in scripts

Every position a script reads is where things are now, near the camera. A script that keeps an Obj, or asks where things are each frame, needs nothing more.

A script that keeps a position in a field of its own, such as where an enemy last saw the player, marks it, and it moves with everything else:

```csharp
[WorldPosition] Vector3 lastSeen;
[WorldPosition] List<Vector3> patrol = [];
```

Anything else that keeps positions, such as a dictionary of them, can move them when it hears of a move:

```csharp
private readonly Dictionary<Obj, Vector3> heardAt = [];

protected override void OnEnable() => Origin.Moved += Moved;
protected override void OnDisable() => Origin.Moved -= Moved;

private void Moved(Vector3 by)
{
	foreach (var obj in heardAt.Keys.ToList())
	{
		heardAt[obj] += by;
	}
}
```

`Origin.Position` says where the world's middle is from the level's own, in whole metres. `Origin.ToLevel` gives a position measured from the level's middle, as a `LevelPosition` of doubles, which a move never changes and which stays exact however far out it is. `Origin.FromLevel` turns one back into a position in the world as it is now.

A save of the scene keeps where things are from the level's middle (see [[Saving the Game]]), so a game saved far out loads in the right place.

## In the editor

The level being edited never moves. While it plays, its middle moves as it would in the game; when play stops, the Scene view's camera comes back where it flew to, measured from the level's middle.

## Not done yet

- Grass, wind, the seasons' patches and a few other looks take their pattern from where they are in the world, so they change a little when the middle moves.
- The middle moves only across the ground, so a camera flying more than a few kilometres up is drawn less exactly.
- Networked games keep their middle where it is.
