---
order: 8
---

A **texture pack** is the pictures a game's blocks and materials are drawn with, chosen by whoever plays it. It is a short list of which picture stands in where: this block's top, that block's sides, this material's colour map. Everything else stays as you made it, so one pack suits every level, and a player's choice goes with them from level to level and is there again the next time they play.

[[Look Packs]] change a level's light. Texture packs change its pictures. A player can wear one of each.

## The packs Voxear comes with

| Pack | What it does |
| --- | --- |
| **Pixel Grain** | Every block of a plain colour drawn as sixteen sharp pixels a side, each a shade of its own |
| **Soft Grain** | A fine, soft grain on every block of a plain colour |

Voxear cannot know your game's blocks, so its own packs name none: they make a picture from each block's colour. A block with a material keeps it.

## Seeing a level under a pack

On the **Lighting** page, under **Look pack**, **Texture pack** shows the open scene under any pack, as a player who picked it would see it. Nothing of it is saved with the scene. Playing starts under the pack chosen the last time you played, as a build would, and the page's pack is back when you stop.

## Making a pack

In the Project panel, right-click a folder and pick **New Texture Pack**. A pack is text:

```toml
format = 1
description = "Hand-drawn blocks, sixteen pixels a side."
pixels = true

[[block]]
block = "Blocks/Grass.block"
top = "Packs/Drawn/grass_top.png"
sides = "Packs/Drawn/grass_side.png"
bottom = "Packs/Drawn/dirt.png"

[[block]]
block = "Blocks/Stone.block"
all = "Packs/Drawn/stone.png"

[[material]]
material = "Materials/Planks.material"
color_map = "Packs/Drawn/planks.png"
```

| Part | What it holds |
| --- | --- |
| `description` | A line for the menu the pack is picked from |
| `pixels` | Draws the pack's colour pictures sharp, each pixel a square of one colour however near, as pixel art is. Left out, they are smoothed |
| `[[block]]` | One block's pictures: `block`, the [[Blocks\|block]] by its path, then `all` for every face, or `sides`, `top` and `bottom` |
| `[[material]]` | One [[Materials\|material]]'s pictures, wherever it is drawn: `material`, then any of `color_map`, `normal_map`, `metallic_map`, `emissive_map` and `height_map` |
| `connected` | On a block or a material: its pictures here are five tiles, and faces drawn with them join (see [[Blocks#Faces that join]]) |
| `overlap` | On a block: where it lies in the order of blocks that meet under this pack, in place of its own (see [[Blocks#Blocks that meet]]). An entry can hold this alone |
| `[grain]` | A speckled picture, made of its colour, for every block of a plain colour the pack names no picture for: `pixels` a side, and `strength` from 0 to 1 |

A face is one picture, its colour. For more, write the face as a table with the names a material uses:

```toml
[[block]]
block = "Blocks/Ore.block"
all = { color_map = "Packs/Drawn/ore.png", emissive_map = "Packs/Drawn/ore_glow.png" }
```

- A block of a plain colour is drawn with the pack's picture as it is.
- A block with a material keeps everything else its material says: its tint, how rough it is, whether it is cut out. Only its pictures change.
- A liquid keeps its own look.
- A block or material the pack names that your game does not have is left out, with a line in the Console, so a pack made for an older version of a game still dresses what it can.

Select a pack in the Project panel and the Inspector shows its description, how many of the blocks and materials it names were found, and its text. Save the file from your text editor and a scene shown under it changes at once.

Every texture pack of a project goes into its builds with its pictures, whether or not a level names it.

## Letting players choose

`TexturePackScreen.Open()` shows a ready-made menu: a row for each pack, the one worn marked, and **None** at the top. A click wears a pack at once. **New Pack** makes a pack of the player's own to start from.

```csharp
public sealed class PauseMenu : Behaviour
{
	protected override void Update()
	{
		if (Keyboard.WasPressed(Key.T))
		{
			TexturePackScreen.Open("Textures");
		}
	}
}
```

For a menu of your own, `TexturePacks` has everything the screen uses:

| | |
| --- | --- |
| `TexturePacks.All` | Every pack there is to wear: Voxear's own, then your project's, then the player's, each by name |
| `TexturePacks.Current` | The pack worn, or `TexturePack.None` |
| `TexturePacks.Use(pack)` | Wears a pack from the next frame, and keeps the choice for the game's next run. Pass `remember: false` to try one without keeping it |
| `TexturePacks.UseNone()` | Takes the pack off |
| `TexturePacks.Folder` | Where a player puts packs of their own |
| `TexturePacks.Changed` | An event: the pack worn changed |
| `TexturePacks.WriteStarter(name)` | Makes a pack for the player to start from, and gives it back ready to wear |
| `pack.Name`, `pack.Description`, `pack.Source` | What a menu shows of a pack, and whether it is built in, the game's or the player's |

The built-in packs are there by name, `TexturePack.PixelGrain` and `TexturePack.SoftGrain`. A script field of type `TexturePack` picks one of the project's in the Inspector.

## Packs players add

Beside the game's saves is a folder named `Textures`:

| | |
| --- | --- |
| Linux | `~/.local/share/<company>/<game>/Textures` |
| Windows | `%LOCALAPPDATA%\<company>\<game>\Textures` |

Each pack is a folder of its own there, holding one `.texturepack` file and its pictures. It is listed under its folder's name, marked **Yours** on the ready-made screen.

- It names blocks and materials by their paths in the game, as your own packs do.
- It names its pictures by their files' names in its own folder. Nothing outside that folder is read.
- It is pictures and settings and nothing else: a pack cannot run code.
- A picture can be up to 4096 pixels a side.

A player does not know your blocks' paths, so the game can write them down. **New Pack** on the screen, or `TexturePacks.WriteStarter("My Pack")`, makes a folder with every block of the game listed, and a small picture of each block of a plain colour to paint over. Blocks with a material are listed too, switched off, for the player to give a picture.

## What a pack cannot do yet

- Be a `.zip`: a player's pack is a folder.
- Change a model's own pictures, unless the model's material is a `.material` file.
- Draw its normal and other maps sharp: `pixels` is for the colour picture.
