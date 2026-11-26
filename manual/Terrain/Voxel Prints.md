---
order: 31
---

A **Voxel Print** (`.voxprint`) is a model made into voxels: a sandbag wall, a crate, a timber revetment or a dugout's entrance, solid all through, in the colours of its pictures. It is printed into an endless [[Voxel Volumes|Voxel Volume]]'s ground where it is placed, as a ball is carved or filled, so it is part of the ground: a ball carved into it later takes a bite out of it and finds more of it inside, not a hollow shell, and a ray meets it as it meets the ground.

## Making one

Right-click a model in the Project panel (a `.gltf`, `.glb`, `.fbx` or `.boxmodel`) and choose **Make Voxel Print**. The print is made beside the model, with the model's name, at 16 voxels a metre, and shown in the Inspector. It is made on a thread of its own while the editor goes on: a scanned piece whose file is 100 MB takes about five seconds, and a note says when it is done.

Each triangle of the model fills the voxels it passes through, in the colour its picture has where the triangle is nearest the voxel's middle; a voxel many triangles pass through takes their average. The voxels inside the skin are then filled, each the colour of the skin nearest it, and every colour is made the nearest of a few. A model's pixels under its material's alpha cut-off leave no voxels, so a torn cloth keeps its holes.

The Inspector shows its size in voxels and in metres, how many voxels are solid, how many colours it has, and how large its file is. **Made From** says how it is made:

- **Model**: the model it is made from.
- **Voxels a Metre**: how small its voxels are: as many as the volumes it goes in, for voxels of the same size as theirs. A print of another size is printed all the same, each of the volume's voxels taking the print's voxel at its middle, or near it, so a thin piece is not lost.
- **Scale**: multiplies the model's size.
- **Colours**: the most colours its voxels are in, up to 128. A volume keeps each print's colours, in each tint it is printed in, together in a bank of 128 of its own; with all 32 banks full, a print's colours are the nearest the volume has.
- **Fill**: what it leaves inside its skin. **Solid** fills wherever the outside can't reach, so a closed model is solid all through. **Sealed** fills too where the skin is round a place on five of its six sides, for a model whose skin is open, as a scan with no underside; an open crate's inside is filled with it, so leave such a model Solid. **Shell** keeps the skin alone, a voxel thick.
- **Pictures**: a picture for each of the model's parts, for a model whose file names none, as one exported without its pictures. A part with neither its own picture nor one here takes its material's colour.

The settings change nothing until **Make Again**, which makes the print from its model as they say. Every volume printing it shows it as it is now.

**Glass and Lights** lists the print's colours. Each can be made **Glass**, seen through as a window is, **Frosted Glass**, more of its own colour, **Light**, glowing as a lantern's flame and lighting what is round it for fifteen metres, or a **Dim Light**; or left **Solid**. A change is kept in the print's file at once, and every volume printing it shows it so: its voxels are as they were. See [[Voxel Volumes]] for how glass and lights are drawn.

A print's file is small: its palette and how it was made, then each brick of eight voxels a side as one byte where it is empty or all one colour, the rest packed with zstd. A sandbag barrier 3.5 m long at 16 voxels a metre is under 2 KB, a timber wall 2 m by 1.7 m about the same. Printing one adds only its name, place and turn to the scene, which is all that goes over the network to a player who joins.

## Printing one

With an endless volume selected, set its **Brush** to **Print**, pick the print in **Brush Print** and how far it is turned in **Brush Turn**: its box shows where the ground under the pointer is, and a click prints it there, its model's origin on the ground. Each print is one step to undo.

From a script, a `VoxelPrint` field is picked in the Inspector like any asset, and printed into every endless volume its box reaches:

```csharp
public VoxelPrint SandbagWall;

// Sandbags along the trench's lip, turned to face the enemy.
Voxels.Print(SandbagWall, lip, turn: 90f);
// Leaning with the slope: degrees about X, Y and Z, as an Obj's rotation.
Voxels.Print(SandbagWall, lip, new Vector3(8f, 90f, 0f));
```

It is printed once the frame is done.

### Fitted to the ground

A print placed on a slope stands on the ground at its origin: on the low side its foot is over the ground, on the high side the ground comes up through its floor. Fitted, the ground inside it is cleared down to its foot, in every column it stands over, and where the ground falls away under it a **footing** reaches down to the ground, as deep as the print is tall at most, in the ground or paving it is given. Only the columns the print stands over are changed, so an L-shaped house gets an L-shaped footing, and the ground round it is left as it was.

```csharp
// A house on the hillside, on an ashlar footing.
Voxels.Print(House, plot, new Vector3(0f, 15f, 0f), fit: true, footing: VoxelMaterial.Ashlar);
```

In the brush, tick **Brush Fit** and pick **Brush Footing**; with `vox editor call voxels.print`, give `"fit": true` and `"footing": "ashlar"`.

### Tinted

A print's colours can be tinted: each is shaded from the tint as it was from the print's own average colour, so one house printed in many paints keeps its shading, its timbers darker than its plaster.

```csharp
// The same house in ochre.
Voxels.Print(House, plot, new Vector3(0f, 15f, 0f), tint: new Color(0.8f, 0.62f, 0.3f));
```

`vox editor call voxels.print` takes `"tint": [204, 158, 76]`.

No grass or flowers grow where a print stands, in its box over any column of it that has a voxel, so an open hut's floor and a crate's inside are bare; nor does a tree whose foot is there or under it, as far below its foot as it is tall, nor one whose foot a change carved out.

Turned other than by quarter turns, or made of voxels smaller than the volume's, a print is looked at round each of the volume's voxels' middles as well as at it, a quarter of a voxel toward its corners: a wall one voxel thick stays whole, its voxels meeting face to face, a little thicker than it was. `vox editor call` makes and prints them too, each print one step to undo:

```
vox editor call voxel_prints.make '{ "model": "Models/Crate.fbx", "voxels_per_metre": 16 }'
vox editor call voxel_prints.make '{ "print": "Models/Crate.voxprint", "fill": "sealed" }'
vox editor call voxel_prints.info '{ "print": "Models/Crate.voxprint" }'
vox editor call voxel_prints.materials '{ "print": "Models/House.voxprint", "materials": [{ "colour": [196, 222, 230], "glass": 14 }, { "colour": [255, 183, 108], "light": 14 }] }'
vox editor call voxels.print '{ "print": "Models/Crate.voxprint", "at": [12, 24, 9], "turn": 30 }'
```

`voxel_prints.materials` makes colours of a print glass or lights, in place of those it had: `glass` says how see-through, from 0 to 15 (14 for a window), and `light` how bright, from 0 to 15, as a block's Light. A colour it leaves out is solid.

`voxel_prints.make` with a `model` makes a new print, beside the model unless `path` says where; with a `print`, it makes that one again as it was made, but for what the call gives. It also takes `scale`, `colours` and `pictures`: `[{ "part": "material0", "picture": "Models/Crate_BaseColor.png" }]`. It makes the print before it answers, so the answer can say what was made; with `"background": true` it answers at once and makes it on a thread of its own, as the Project panel's menu does, and `voxel_prints.info` finds it once it is made.

The scene keeps each print with the volume's other changes, in order:

```toml
[[obj.voxel_volume.edits]]
print = "Models/Crate.voxprint"
at = [12.0, 24.0, 9.0]
rotation = [0.0, 30.0, 0.0]

[[obj.voxel_volume.edits]]
print = "Models/House.voxprint"
at = [40.0, 23.0, 12.0]
fit = true
fill = "ashlar"
tint = [204, 158, 76]
```

## What it doesn't do yet

- A volume of a set size can't take prints, only an endless one.
