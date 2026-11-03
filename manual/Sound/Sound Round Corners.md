---
order: 3
---

With occlusion on, a wall between a sound and the listener muffles it, and a concrete wall all but silences it (see [[Sounds and Audio Sources]]). In a real building, though, you would still hear a radio in the next room through the open door, and you would hear it from the door's side. **Pathing** gives your game that: a sound behind a wall comes round it the way sound would, through doorways and along corridors, and is heard from where it comes in.

Pathing needs Steam Audio, as occlusion does, and paths baked for the level in the editor.

## Baking a level's paths

Open the level, then run **Bake Sound Paths** from the command palette (Ctrl+K). You can also select the level in the Project panel and click **Bake Paths** under **Sound Paths** in the Inspector. The level must be open to bake it.

Steam Audio stands probes on every floor of the level, a metre and a half up and two metres apart, then works out which probes can see each other, and the shortest path between every two. The walls and floors are the same as for occlusion: the level's colliders that stay put, those without a Rigidbody and not triggers. A small level bakes in a second or two, and a large one can take minutes. The bake runs in the background, and the status bar shows how far it has got.

When it's done, the paths are saved beside the level, named after it: `Levels/Main.level` gets `Levels/Main.paths`. The Inspector's **Sound Paths** says whether a level is baked, and how large the file is. Bake again after you move walls, floors or doorways, as the paths don't change by themselves.

A few things to know:

- Probes stand on floors, so a level needs a collider underfoot wherever the listener and the sounds go. Where there's no floor, there are no paths.
- A level with a lot of floor gets its probes further apart, to keep the bake to about 4,000 probes.
- Paths longer than 100 metres are left out. Few sounds carry that far.

## Turning pathing on for a sound

Pathing is set on each Audio Source. Set **Occlusion** to Raycast or Volumetric, then turn on **Pathing**, which shows under it.

| Setting | What it does |
| --- | --- |
| Pathing | Whether the sound also comes round the walls along the level's baked paths |

While the game runs, Voxear finds the path from the probe nearest the sound to the probe nearest the listener, every frame. The more of the sound the walls stop, the more of it comes round along the path instead. The more sharply the path bends, the duller the sound becomes, as the high notes bend round corners least. The sound is heard from the way the path comes in: through a doorway on your left, it comes from your left.

A sound in plain view is heard straight, as before. With **Transmission** on as well, a sound behind a thin wall comes partly through the wall and partly through the doorway.

In a level without baked paths, or without Steam Audio, Pathing changes nothing.

## From scripts

`AudioSource.Pathing` turns it on or off from code:

```csharp
namespace MyGame;

public sealed class Radio : Behaviour
{
	protected override void Start()
	{
		var radio = Obj.AudioSource;
		radio.Occlusion = SoundOcclusion.Raycast;
		radio.Pathing = true;
	}
}
```

## In a build

A build packs each level's `.paths` file with the level, so a built game hears the same paths as the editor does.
