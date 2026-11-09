---
order: 19
---

A **data asset** is a file of values, such as a rifle's damage and range, that you edit in the Inspector and share between Objs. A script gives it its shape: a class deriving from `Definition`, whose fields are the values each file holds.

```csharp
[CreateAsset("Weapons/Weapon Stats")]
public sealed class WeaponStats : Definition
{
	public float Damage = 10f;
	public float Range = 50f;
	public List<int> Shots = [];
}
```

A Definition's fields follow the rules a behaviour's do: public fields, or private ones marked `[SerializeField]`, of any type [[Fields in the Inspector]] lists, with the same attributes. A Definition has no Obj and no `Update`: it holds values, and any methods you give it to work with them.

## Making a data asset

Right-click in the Project panel and choose **New Data Asset…**. The list shows each Definition in your scripts, in the folders its `[CreateAsset]` path names. A Definition without `[CreateAsset]` is under **Scripts**, in a folder for its namespace, and `[CreateAsset("")]` leaves one out. Pick one, and a new `.asset` file named after it, such as `New Weapon Stats.asset`, appears in the folder, ready to rename.

Select the file and the Inspector shows its fields, as it shows a behaviour's, under the file's name and its Definition's. Each change goes into the file as you make it, so a data asset has nothing to save, and nothing to undo.

The file names its Definition by its full name, then the values that differ from the script's:

```toml
type = "Hello.WeaponStats"

[fields]
damage = 35.5
shots = [1, 2, 3]
```

A field the file leaves out has the value written in the script.

## A field that picks a data asset

A behaviour field of a Definition's type picks one of its data assets:

```csharp
public sealed class Rifle : Behaviour
{
	public WeaponStats Stats;

	protected override void Start()
	{
		Log.Info($"{Obj.Name} does {Stats.Damage} damage");
	}
}
```

The field's list holds the data assets of that Definition, and of Definitions deriving from it, and no others. Pick one from the list, or drag one from the Project panel onto the field.

Every field that picks the same file shares one instance of it, loaded the first time something asks for it. A value your code changes on it is changed for every Obj holding it, until the game stops. The file changes only when you edit it in the Inspector.

## Loading a data asset from code

`Definition.Load<T>` loads a data asset by its path, the same instance a field gives:

```csharp
var rifle = Definition.Load<WeaponStats>("Weapons/Rifle.asset");
```

It gives back null, with an error in the Console, when the project has no such file or the file holds another Definition. `Name` is the file's name, without its folder or extension, and `Path` is its path in the project.

A build ships each data asset a field, a scene or another data asset names. A path your code puts together, such as `$"Weapons/{name}.asset"`, is one the build cannot see, so name the files with `[ContentRoot]`, as [[What Goes into a Build]] shows:

```csharp
[ContentRoot("Weapons/*.asset")]
public sealed class Armoury : Behaviour
{
}
```
