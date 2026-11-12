---
order: 7
---

A **look pack** is how a game looks, chosen by whoever plays it: a warmer sun, tinted shade, clouds built of cubes, more glow, block faces cut into little cubes. It is a small file of lighting settings laid over whatever lighting a level has. The level keeps its time of day, its [[Weather]] and everything the pack leaves out, so one pack suits every level of a game, and a player's choice goes with them from level to level and is there again the next time they play.

[[Lighting Settings]] are a level's lighting, made by you. A look pack goes on top of them, picked by the player.

## The packs Voxear comes with

| Pack | What it does |
| --- | --- |
| **Sunlit** | Soft sunlight, blue shade, a little glow and rich, clean colour |
| **Cube Clouds** | The level's clouds built of big soft cubes, their square shadows crossing the ground |
| **Little Cubes** | Every block's faces cut into little cubes, with clouds built of cubes to match |
| **Painted** | Rich colour and soft shapes, as in a picture book: teal shade, golden light and a glow round bright things |
| **Old Film** | Faded colour, deep contrast, dark corners and the fringing of an old lens |
| **Plain** | Nothing over the picture: no glow, no mist, no dark corners, and plain faces on every block |

A pack changes only what is there to change: Cube Clouds does nothing over a level with a clear sky, and Little Cubes changes only [[Block Worlds]].

## Seeing a level under a pack

On the **Lighting** page, under the scene's lighting, **Look pack** shows the open scene under any pack, as a player who picked it would see it. It is a way to check your levels against the packs your game offers: nothing of it is saved with the scene, and the page's settings go on showing the level's own lighting.

## Letting players choose

`LookPackScreen.Open()` shows a ready-made menu: a row for each pack with what it looks like, the one worn marked, and **None** at the top. It sits down the left of the screen so the game shows beside it, and a click wears a pack at once.

```csharp
public sealed class PauseMenu : Behaviour
{
	protected override void Update()
	{
		if (Keyboard.WasPressed(Key.L))
		{
			LookPackScreen.Open("Looks");
		}
	}
}
```

For a menu of your own, `LookPacks` has everything the screen uses:

```csharp
foreach (var pack in LookPacks.All)
{
	menu.Add(new Button(pack.Name)).Clicked += () => LookPacks.Use(pack);
}
```

| | |
| --- | --- |
| `LookPacks.All` | Every pack there is to wear: Voxear's own, then your project's, then the player's, each by name |
| `LookPacks.Current` | The pack worn, or `LookPack.None` |
| `LookPacks.Use(pack)` | Wears a pack from the next frame, and keeps the choice for the game's next run. Pass `remember: false` to try one without keeping it |
| `LookPacks.UseNone()` | Takes the pack off: every level as it was made |
| `LookPacks.Folder` | Where a player puts packs of their own |
| `LookPacks.Changed` | An event: the pack worn changed |
| `pack.Name`, `pack.Description`, `pack.Source` | What a menu shows of a pack, and whether it is built in, the game's or the player's |

The built-in packs are there by name, such as `LookPack.Sunlit` and `LookPack.LittleCubes`. A script field of type `LookPack` picks one of the project's in the Inspector.

The choice is kept beside the game's saves, and the game starts wearing it. In the editor, playing starts under the pack chosen the last time you played, as a build would. A pack that is no longer there is forgotten.

## Making a pack

In the Project panel, right-click a folder and pick **New Look Pack**. A pack is text:

```toml
format = 1
description = "Warm light, soft blue shade and clouds built of cubes."

[sun]
tint = [1.0, 0.96, 0.88]
shadow_tint = [0.05, 0.08, 0.16]

[sky.clouds]
kind = "blocks"

[post.bloom]
intensity = 0.14

[blocks]
voxels = 8
```

| Part | What it holds |
| --- | --- |
| `description` | A line for the menu the pack is picked from |
| `[sun]` | How the sun looks, as lighting settings write it: `temperature`, `tint`, `strength`, `softness`, `shadow_tint`, `shadow_distance`, `shadow_split`. A `tint` colours the level's own sun; the others take the place of the level's |
| `[sky]` | The sky, as a scene's `[sky]` writes it, and under `[sky.clouds]` its clouds (see [[The Sky]]) |
| `[post]` | Post-processing, as a scene's `[post]` writes it: glow, colour, mist, dark corners and the rest (see [[Post-Processing]]) |
| `[blocks]` | How every Block World's faces are drawn: `voxels`, the small cubes along a block's side, 0 for plain faces, and `relief`, how many deep the deepest sink (see [[Block Worlds#Faces of small cubes]]) |
| `look` | A built-in look, such as `"storybook"`, to start from **in place of** the level's own lighting, for a pack that makes every level look one way. Left out, the pack is laid over the level's |

Only what you write changes. A pack cannot set the weather: that is the level's. A setting there is not, or a misspelt one, is reported in the Console and the pack is not worn.

Select a pack in the Project panel and the Inspector shows its description and which parts it changes, over its text. Save the file from your text editor and a scene shown under it changes at once.

Every look pack of a project goes into its builds, whether or not a level names it, since players pick from all there are.

## Packs players add

A player can add packs of their own. Beside the game's saves is a folder named `Looks`:

| | |
| --- | --- |
| Linux | `~/.local/share/<company>/<game>/Looks` |
| Windows | `%LOCALAPPDATA%\<company>\<game>\Looks` |

Any `.lookpack` file there is listed with the game's, marked **Yours** on the ready-made screen. It is a file of settings and nothing else: a pack cannot run code or read anything. **Refresh** on the screen lists a file just added.

## What a pack cannot do yet

- Change water: the sea, pools and water blocks keep the look their level gives them.
- Bring pictures of its own, such as a lookup table for colour or new pictures for blocks.
- Run a shader of its own over the picture.
