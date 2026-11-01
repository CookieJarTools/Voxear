---
order: 4
---

A behaviour's fields show in the Inspector, so you can set them for each Obj without changing the code. The values you set are saved in the scene.

```csharp
public sealed class Door : Behaviour
{
	[SerializeField] private float openSpeed = 2f;
	[SerializeField] private Obj handle;
	public bool StartsOpen;
}
```

The Inspector writes each name out as words: **Open Speed**, **Handle**, **Starts Open**.

## Which fields the Inspector shows

- **Public fields**, such as `StartsOpen`.
- **Private fields marked `[SerializeField]`**, such as `openSpeed`. Most scripts use these, so other code cannot change them by mistake.

Properties never show, and neither do `readonly` fields or fields marked `[NonSerialized]`.

The value written in the code, such as `= 2f`, is the default. Right-click a field and choose **Reset to the script's value** to go back to it.

Values are set just before `Awake`, so by the time any of your methods run, the fields hold what the Inspector says.

## What a field can hold

| Type | In the Inspector |
| --- | --- |
| Numbers, `bool`, `string` | A box to type in, or a tick |
| An `enum` | A list to pick from |
| `Vector2`, `Vector3`, `Vector4` | A box for each part |
| `Quaternion` | Three angles, in degrees |
| `Color` | A colour swatch, which opens a colour picker |
| Arrays and `List<T>` | Each item, with buttons to add and remove items |
| A `[Serializable]` class or struct | Its own fields, indented |
| `Obj` | Another Obj in the same scene |
| A behaviour type, such as `Door` | An Obj in the same scene that has that behaviour |
| `Prefab` | A prefab in the project |

Click a field that refers to an Obj or a prefab to pick from a list, or drag an Obj from the Hierarchy, or a prefab from the Project panel, onto the field.

An Obj field can only point at an Obj in the same scene, or in the same prefab. Inside a prefab, each instance's field points at that instance's own copy.

> [!TIP]
> A `Prefab` field loads its prefab when the level loads. For a prefab the game might never need, use `Ref<Prefab>` instead, and call `Load()` on it when you do need it.

## Arranging fields with attributes

Attributes on a field change how the Inspector shows it. A few of the most useful:

```csharp
[Range(0f, 10f)]
public float Speed = 2f;

[Suffix("m/s")]
public float TopSpeed = 8f;

[Tooltip("How long the door takes to open")]
public float OpenTime = 1f;
```

| Attribute | What it does |
| --- | --- |
| `[Range(min, max)]` | A slider between two values |
| `[MinValue(n)]`, `[MaxValue(n)]` | Keeps typed values in bounds |
| `[Suffix("m")]` | Shows a unit after the value |
| `[Tooltip("…")]` | Words that show when the pointer rests on the field |
| `[Title("…")]` | A heading over the field |
| `[LabelText("…")]` | A label of your own in place of the field's name |
| `[ReadOnly]` | Shows the value, greyed out |
| `[HideInInspector]` | Hides the field |
| `[Multiline(lines)]` | A taller box for long text |
| `[FoldoutGroup("name")]`, `[BoxGroup("name")]` | Puts fields together in a section that folds, or a box |
| `[TabGroup("name")]` | Puts fields on tabs |
| `[ShowIf(nameof(Field))]`, `[HideIf(…)]` | Shows the field only when another field is set |
| `[InfoBox("…")]` | A note above the field |
| `[Required]` | Shows an error while the field is empty. Builds warn about it too |
| `[Button]` | On a method that takes nothing: a button that runs it while you edit |

The Code Reference, in VoxDocs, has a page for every attribute: look in the **Inspector** folder under **Voxear**. When no attribute shows a field the way you want, write your own drawer: see [[Custom Drawers]].

## When a script's fields change

When you save a script, the editor builds it and shows its fields as they now are. If the build fails, the Inspector keeps showing the fields from the last good build, and the Console says what is wrong.

Renaming a field loses the values saved under its old name. The scene warns about the old name the next time it loads.

Next: [[Commands]].
