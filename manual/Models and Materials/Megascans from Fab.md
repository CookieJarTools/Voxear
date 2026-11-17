---
order: 9
---

**Megascans** are scanned rocks, plants and ground surfaces, free on **Fab**, an online asset store. Voxear brings one into your project in one step: download it in your browser, then drop the zip on the Project panel. You get a folder with its textures, a material and its models. A model that comes with its own levels of detail also gets a prefab, ready to drag into a scene.

## Downloading from Fab

Fab has no way for another program to sign in or download for you, so you download in your own browser, signed in to Fab there. Voxear never sees your Fab account.

1. Right-click in the Project panel and choose **Open Fab**. Fab's Megascans page opens in your browser.
2. Pick an asset and add it to your Fab library. Adding it is what gives you the licence to use it, even when it is free.
3. Download it as **FBX** or **glTF**; either works. For the quality, **High** suits most things: 4K textures, and a model for each level of detail. **Raw** is the scan as it was made: one very detailed model with 8K textures, which takes far more memory and is slower to import, and which can be open behind, scanned from the front only.

## Importing a download

Drag the zip from your file manager onto the Project panel. There is no need to unzip it, though a folder you have unzipped works too. You can also right-click and choose **Import New Asset…**, then pick the zip.

Voxear knows a Megascans download by the names of its pictures, such as `Mossy_Rock_tdcnfcdr_High_4K_BaseColor.jpg`. It imports the download in the background, so you can keep working. The status bar says when it is done, and the Project panel selects what it made.

Each asset gets a folder of its own, named after it, such as `Megascans/Mossy Rock`:

- Dropped on a folder, or on the files while the Project panel shows a folder, the asset goes into that folder.
- Dropped anywhere else, or on the files of the project's top folder, it goes into a `Megascans` folder at the top of the project.

Importing the same asset again makes a second folder, `Mossy Rock 1`, beside the first. A download whose pictures are not named this way is copied in as ordinary files, as any other drop is.

## What an import makes

| File | What it is |
| --- | --- |
| `Mossy Rock.material` | The asset's material, reading the textures below. **Two sided** for a plant, and for a model open behind |
| `Textures/Mossy Rock Colour.jpg` | Its colour. For a plant, a `.png` with its see-through parts in the alpha |
| `Textures/Mossy Rock Normal.jpg` | Its normal map |
| `Textures/Mossy Rock Metallic.png` | Its roughness, metalness and ambient occlusion in one picture, as a material's **Metallic Map** reads them |
| `Textures/Mossy Rock Height.jpg` | A surface's height, for **Displacement** |
| `Mossy Rock_LOD0.fbx`, `Mossy Rock_LOD1.fbx`… | Its models, one for each level of detail, drawn with the material |
| `Mossy Rock.prefab` | Its models in a LOD Group: the one to drag into a scene |

Every file comes with its `.meta`, so the material, the models and the prefab find each other straight away. Move or rename the folder as you like; the references keep working.

### Rocks and other models

Drag `Mossy Rock.prefab` into the Scene view. Its **LOD Group** shows the most detailed model up close and the simpler ones as it gets smaller on screen; see [[Levels of Detail]]. The group's levels are the download's own, so the models in it do not make LODs of their own.

A model that comes as one file gets no prefab: drag the model itself into the scene, and Voxear makes its LODs as it does for any model.

Some scans are only the front of something, such as a window frame cut from a wall, with nothing behind. Seen from behind, such a model would show only its outline, so the import turns on **Two sided** in its material, and its back is drawn as the inside of its front. A model closed all round keeps its material one-sided, which costs less to draw.

### Plants

A plant's leaves are cut out of their cards by the plant's opacity. Its material has **Alpha clip** on, at a **Threshold** of 0.5, **Two sided** on, so each card shows from behind too, and **Foliage** on, so the leaves are lit as leaves and sway in the wind; see [[Foliage]] and [[Wind]]. Its models have **Wind masks** set to **Generate**.

A plant with several variations, such as `VarA` and `VarB` in the download, gets a prefab for each: `Tufted Grass A.prefab`, `Tufted Grass B.prefab`. Scatter them with a Foliage to make a meadow.

### Surfaces

A surface is textures with no model. Drag `Mossy Floor.material` onto an Obj in the Scene view to see it there. The material keeps the surface's height map. To raise the surface by it, type how far in **Displacement**; see [[Displacement]]. Some downloads say how deep their scan is, and Displacement is set to that already.

## Watching your Downloads folder

The editor can pick up downloads by itself, so you do not drag each one in:

1. Open **Preferences**, pick **External Tools**, and click **None** beside **Watch for downloads**.
2. Pick the folder your browser saves downloads into, such as `Downloads`.

From then on, each Megascans zip that finishes downloading there is imported into `Megascans` when you come back to the editor. Only downloads that arrive while the project is open come in. Zips already in the folder are left alone, so drag older ones in yourself. Click the cross beside the folder to stop watching. Each project keeps its own folder to watch.

## What an import leaves out

- **EXR pictures.** Voxear reads PNG and JPEG pictures only. A surface whose only height map is an EXR comes in without one. The Console lists each file left out.
- **Maps no material reads**, such as cavity, bump, specular, translucency and fuzz.
- **OBJ and USD models.** Download FBX or glTF instead.

## Megascans in your game

Fab's licence lets you use Megascans in a game made with any engine, Voxear included. The licence asks you to:

- ship them only inside your game, never as files on their own. Keep them out of public repositories; a private one shared with your team is fine.
- keep players from taking them out of your game's files.
- leave them out of level editors or other tools you give players that could export them.

Read Fab's licence for the full terms before you ship.
