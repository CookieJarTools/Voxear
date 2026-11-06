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
| `LocalizedString` | A key from your string tables, picked from a list beside its text (see [[Translating Your Game]]) |
| An `enum` | A list to pick from |
| `Vector2`, `Vector3`, `Vector4` | A box for each part |
| `Quaternion` | Three angles, in degrees |
| `Color` | A colour swatch, which opens a colour picker |
| Arrays and `List<T>` | A box of its items, each with a grip to drag it to a new place and a cross to take it out, and **+** under it to add one |
| A `[Serializable]` class or struct | Its own fields, indented |
| `Obj` | Another Obj in the same scene |
| A behaviour type, such as `Door` | An Obj in the same scene that has that behaviour |
| `AudioSource` | An Obj in the same scene that has an Audio Source |
| `Collider`, `Rigidbody`, `CharacterController`, `Joint`, `Collider2D`, `Rigidbody2D` | An Obj in the same scene that has one |
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
| `[ColorUsage(hdr: true)]` | A `Color` that can glow brighter than white: its picker adds an Intensity in stops |
| `[FoldoutGroup("name")]`, `[BoxGroup("name")]` | Puts fields together in a section that folds, or a box |
| `[TabGroup("name")]` | Puts fields on tabs |
| `[ShowIf(nameof(Field))]`, `[HideIf(…)]` | Shows the field only when another field is set |
| `[InfoBox("…")]` | A note above the field |
| `[Required]` | Shows an error while the field is empty. Builds warn about it too |
| `[Button]` | On a method that takes nothing: a button that runs it while you edit |

An HDR colour keeps its intensity in its channels, times two to that many stops, so its brightest channel is over 1 once it glows. `Color.Red.WithIntensity(2f)` is red four times as bright, and `Intensity` reads the stops back.

The Code Reference, in VoxDocs, has a page for every attribute: look in the **Inspector** folder under **Voxear**. When no attribute shows a field the way you want, write your own drawer, or an inspector that draws the whole behaviour: see [[Custom Drawers]].

## Editing fields in the Scene view

Some values are easier to set by dragging them where they belong than by typing numbers. A handle attribute puts a field in the Scene view, as dots to drag, while its Obj is selected:

```csharp
[PositionHandle] public Vector3 SpawnPoint = new(0f, 0f, 2f);
[RadiusHandle] public float HearingRange = 8f;
[PositionHandle(Space = HandleSpace.World)] public List<Vector3> Patrol = [];
```

| Attribute | On a | What you drag |
| --- | --- | --- |
| `[PositionHandle]` | `Vector3`, or a list of them | A point. A list shows each point, joined in order as a path |
| `[RadiusHandle]` | `float` | The edge of a circle around the Obj, its radius in metres |
| `[DirectionHandle]` | `Vector3` | The tip of an arrow from the Obj, for a direction and its length |
| `[BoundsHandle]` | `Vector3` | The faces of a box around the Obj, its size in metres |
| `[ArcHandle]` | `float` | The end of an arc around the Obj's front, its angle in degrees |
| `[SliderHandle]` | `float` | A point along one of the Obj's axes, its distance in metres |

Points and directions move and turn with the Obj, unless you give them `Space = HandleSpace.World`. Every handle takes a `Color`, such as `Color = "#ff8800"`, and `[ArcHandle]` a `Radius` and `[SliderHandle]` an `Axis`.

Each drag is one step to undo. Hold **Ctrl** while dragging to snap: points to the snap distance shown in the Scene view, other values to tenths of a metre, and angles to 5°. The handles show while you edit, not while the game plays, and the move, rotate and scale handles sit on top of them. Choose the Select tool to reach a dot underneath.

## Where Add Component lists a script

Add Component lists your scripts under **Scripts**, in a folder for each part of their namespace past the one all your scripts share. `[AddComponentMenu]` puts one where you say instead, as in Unity:

```csharp
[AddComponentMenu("Effects/Camp Fire")]
public class CampFire : Behaviour { }
```

The last part is the name it shows. `[AddComponentMenu("")]` leaves a script out, for one only ever added from code.

## When a script's fields change

When you save a script, the editor builds it and shows its fields as they now are. If the build fails, the Inspector keeps showing the fields from the last good build, and the Console says what is wrong.

Renaming a field loses the values saved under its old name. The scene warns about the old name the next time it loads.

Next: [[Commands]].
