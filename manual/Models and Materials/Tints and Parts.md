---
order: 10
---

One model can be drawn in many colours, and an Obj can change which model it draws while the game runs. Together they make a character out of parts: one grey hair model tinted any hair colour, one tunic dyed any dye, and a script that puts on the parts a player picks.

## Tinting a model

A **Tint** is a colour that multiplies everything an Obj's model draws. White leaves the model as it was made, and any other colour darkens it towards that colour. It belongs to the Obj, not to the model or its materials, so two characters drawing the same hair can have different hair colours.

Select an Obj with a Model Renderer and click the **Tint** row in the Inspector to pick its colour.

Tinting works best on a model made light grey or white where it should take the colour, and in its own colours where it should not. A grey hair model tinted brown is brown hair, and the same model tinted black is black hair. Details painted darker grey stay darker in every colour.

A box model keeps its look under a tint. Its pixels stay sharp, its holes stay holes, and far away, where its layers lie flat, it is tinted just the same. The tint colours the cubes of its palette and the pixels of its sheet alike.

In a scene or prefab file, the tint is written under the model as red, green and blue from 0 to 1. It is left out when it is white:

```toml
[obj.model]
model = "bb0838d2eb8ddb05652dd3fc29f94bc2"  # Characters/Parts/Hair/Long.boxmodel
tint = [0.42, 0.29, 0.18]
```

## Three tints, and colours that keep their own

A model can take up to three tints, each on the parts of it that should take that colour, while other parts keep their own colours: a coat dyed one colour, its lining another, and its belt and buckle as they were painted. The Inspector shows **Tint 2** and **Tint 3** on a Model Renderer whose model says where they fall.

A box model says so colour by colour. In the [[Box Models|Model window]], pick a colour to paint or sculpt with, then choose in the bar which tint that colour takes:

| Choice | What a pixel or cube of that colour does |
| --- | --- |
| Tint | Takes the first tint, as every colour does until told otherwise |
| Tint 2 | Takes the second tint |
| Tint 3 | Takes the third tint |
| Own | Keeps its own colour, whatever the tints |

The choice belongs to the colour, so every pixel and cube of it follows, and painting with it later does too. In the file it is a table of the colours that take another tint than the first:

```toml
[tints]
8a6a3a = "none"     # the belt keeps its colour
d8d8d8 = "second"   # the lining takes the second tint
```

Any other model says it with a **tint mask**, a picture laid over its material as its colour map is: red where the first tint falls, green the second, blue the third, and black where it keeps its own colour. Pick it as the material's **Tint Mask** (see [[Materials]]). Shades between mix the tints, so a mask can fade one into another.

In a scene or prefab file the other tints are written beside the first:

```toml
[obj.model]
model = "8f3e2a1c4b5d46e7a9c0b1d2e3f4a5b6"
tint = [0.31, 0.22, 0.45]
tint2 = [0.85, 0.8, 0.7]
```

## From a script

`Obj.Model` is the Obj's Model Renderer. Set its `Tint` to colour it, and its `Model` to draw another model:

```csharp
public sealed class Hair : Behaviour
{
	public ModelAsset Long;
	public ModelAsset Short;
	public Color Colour = Color.Hex("#6b4a2f");

	protected override void Start()
	{
		var model = Obj.Model;
		model.Model = Long;
		model.Tint = Colour;
	}
}
```

A `ModelAsset` field is picked in the Inspector, from the project's models, or dragged there from the Project panel. It keeps the model's ID, so the field still points at the model when the file is renamed or moved.

| Member | What it is |
| --- | --- |
| `Model` | The model it draws. Set another to draw that one from this frame |
| `Tint` | The colour that multiplies everything it draws, white for none |
| `GetTint(slot)`, `SetTint(slot, colour)` | One of its three tints, by `TintSlot.First`, `Second` or `Third` |
| `LodBias` | Shifts which level of detail each part draws, as the Inspector's **LOD bias** |
| `PartCount` | How many parts it draws |
| `Follows` | The Obj whose skeleton poses it, as **Follow parent** does; `Obj.None` for none |
| `IsValid` | False when the Obj has no Model Renderer, or is gone |
| `Remove()` | Takes the Model Renderer off the Obj |

`Obj.AddModel(model)` gives an Obj that draws nothing a Model Renderer drawing that model, and returns it. On an Obj that has one already, it draws the model in place of the last.

When the model changes:

- The tint and the LOD bias stay as they were.
- What the Inspector changed of the last model's parts, such as a part's material, goes, as the new model's parts are other parts.
- If the new model has the same bones as the last, as two parts made on one rig do, the bones keep their pose, so nothing jumps. A model with other bones stands at rest until whatever animates it poses it.
- A model that cannot be read, or an asset that is not a model, is written to the Console, and the Obj goes on drawing the model it had.

On a server, where nothing is drawn, the Obj still takes the new model's bones and clips, so whatever animates it goes on working.

## A character made of parts

Give each part a character can wear an Obj of its own under the character, each with a Model Renderer: one for the body, one for the hair, one for each piece of clothing.

Build every part on the body's rig, its bones named as the body's are, and tick **Follow parent** on each part's Model Renderer. Each frame the part is posed by the skeleton of the Obj it is under, bone by bone by name, so it walks, runs and waves with the body, and whatever animates the body animates every part. Bones a part has that the body has not, as a cape's or a ponytail's, stand at rest from the bones they hang from. What the body hides from the cameras, as a [[First-Person View]] hides its head, each part hides too.

Then a choice on the creation screen is one line for each part:

```csharp
public sealed class Wardrobe : Behaviour
{
	public Obj Hair;
	public Obj Top;
	public ModelAsset[] Hairs = [];
	public ModelAsset[] Tops = [];

	public void Wear(int hair, Color hairColour, int top, Color dye)
	{
		var hairModel = Hair.Model;
		hairModel.Model = Hairs[hair];
		hairModel.Tint = hairColour;
		var topModel = Top.Model;
		topModel.Model = Tops[top];
		topModel.Tint = dye;
	}
}
```

Model each part grey, so its tint picks its colour, and a few dozen models do for every colour a player can pick.

A part that is not under the body follows it from a script: `part.Model.Follows = body`. Setting it to the part's parent is the same as ticking **Follow parent**, and `Obj.None` stops it, leaving the part as it was last posed. In a scene or prefab file it is `follow_parent = true` under `[obj.model]`.

> [!NOTE]
> A `ModelRenderer` is a value: keep it in a local to set it, as `var model = hair.Model; model.Tint = colour;`, since C# cannot set a property on a value it was handed straight back.
