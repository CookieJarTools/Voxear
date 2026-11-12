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

Past a few kilometres, things that move begin to shake, and physics loses its footing. So in a world that goes on without end, the engine can now and then move everything back towards the camera by whole metres, keeping the camera near the middle. This is the **floating origin**: the middle of the world floats along with the camera, and the player never sees it move.

## Switching the floating origin on

The floating origin is off until a project switches it on, in **Project Settings ▸ World ▸ Floating Origin**:

| Setting | What it does |
| --- | --- |
| Mode | **Off** keeps the middle where the level puts it. **Endless Worlds** moves it only in levels with ground without end. **Every Level** moves it in all of them |
| Distance | How far the camera goes from the middle, any way, up and down too, before it moves: 2048 m |
| Step | How far it moves at a time: 1024 m, a whole number of terrain tiles and block chunks, so ground lines up the same after a move |

Ground without end is an [[Endless Worlds|endless terrain]] or an endless [[Block Worlds|Block World]]. Once the camera is further than the distance from the middle, everything moves back by whole steps, between two frames. It moves up and down as well as across, so a level high above or far below the level's middle is as exact as one at it; the sky and the fog still measure heights from the level's ground. The settings are kept in the project's `voxear.toml`:

```toml
[floating_origin]
mode = "endless"
distance = 2048
step = 1024
```

A game whose levels stay within about 8 km of their middle needs none of this: positions there are exact to half a millimetre.

## What moves with the middle

Everything moves together, so nothing the player sees or hears changes:

- every Obj, by its Transform;
- physics: every body keeps how fast it goes, a sleeping one stays asleep, and characters keep their footing;
- sound: no voice moves or bends in pitch;
- smoke, mist, debug drawing and what the renderer remembers from the frame before, so there is no blur and nothing flickers;
- ground made round the camera, which carries on from where it was without making anything again.

A prop left far behind comes back exactly where it was put, however far the camera goes and returns. Loading a level starts again from its own middle, and Objs that stay through it, such as the player, move with the world.

## Patterns far out

Grass, wind gusts, the seasons' patches, puddles and glitter on the ground, foam, caustics and streams on water, lava's plates, glows' flicker, the sun's cookie and its shadows' texels all take their pattern from where things are measured from the level's middle. They stay where they are when the middle moves, and are as fine 100 km out as at the middle.

Such patterns repeat every 4096 m, or 65 536 m for the broadest, along straight lines across the level. Grass and other patterns made of random cells show no line there; a smooth one, such as the seasons' patches, may show a faint edge.

A [[Custom Shaders|custom shader]] reads its patterns from `levelPosition` in the same way, rather than from `position`.

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

## In a networked game

A server keeps its middle where the level puts it, as its world is every player's; a host does too. Each player's own machine moves its middle with its own camera, so two players far apart each see their part of the world exactly.

Positions go between machines measured from the level's middle: each machine adds its own middle's place as it sends a position, and takes it away as it reads one. Players and the server never need to know where each other's middles are. Positions up to 8 km from the level's middle cost what they always did, a millimetre's steps in 24 bits an axis and a bit more for being near; further out, they take 16 bits an axis more, out to half a million kilometres.

A script's own positions go the same way when they are marked:

```csharp
[Replicated, WorldPosition] public partial Vector3 Rally { get; set; }

[ServerRpc]
private void Signal([WorldPosition] Vector3 at) { }
```

The server's own physics works from the level's middle, so what it moves more than about 10 km from there is moved less exactly, as in a game with no floating origin. Keep a networked game's play within that, or its server's Objs near the middle.

## In the editor

The level being edited never moves. While it plays, its middle moves as it would in the game; when play stops, the Scene view's camera comes back where it flew to, measured from the level's middle.

## Not done yet

- With the GI's **Probe Spacing** other than a whole metre, a half or a quarter of one, or two metres, its probes are placed again when the middle moves, and the light settles again over a moment.
- A server's physics stays at the level's middle, so players more than about 10 km from it are moved less exactly on the server.
