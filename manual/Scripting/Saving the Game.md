---
order: 13
---

`Save` keeps your game's progress between runs. Give it a name for the save, called a **slot**, and the values to keep. Nothing needs setting up first.

```csharp
namespace MyGame;

public sealed class GameData
{
	public int Level = 1;
	public float Health = 100;
	public Vector3 Position;
	public List<string> Items = [];
}
```

```csharp
Save.Write("slot1", data);

var data = Save.Read<GameData>("slot1");
```

Each slot is one file in the player's save folder. A save is written whole beside the old one, and only then put in its place, so if the game crashes or the disk fills up halfway through, the save from before is still there.

## Values a save can keep

Almost anything can go in a save:

| What | How it is kept |
| --- | --- |
| Numbers, `bool`, `string`, `char` and enums | As they are. Enums are kept by name, so reordering an enum doesn't change old saves. |
| `Vector2`, `Vector3`, `Vector4`, `Quaternion`, `Color` and `Matrix4x4` | As lists of numbers |
| `DateTime`, `TimeSpan` and `Guid` | As text |
| Arrays, `List`, `HashSet`, `Queue`, `Stack` and `Dictionary` | Item by item |
| Your own classes, structs and records | Field by field (see below) |
| `Obj` | As the scene it was loaded from and its place there, so it finds the same Obj in a later run |
| A behaviour another value points to | As the Obj it is on |
| Assets such as `Prefab` and `Sound`, and `Ref<T>` | As the asset's GUID, so moving or renaming the file doesn't break the save |

A class, struct or record saves its public fields and properties, and any private field marked `[SerializeField]`, as the Inspector shows them. To leave one out, mark it `[DontSave]`, or `[NonSerialized]` for a field. Delegates and events are always left out.

When a class is read back, it's made with its constructor, so any field the save doesn't have keeps the value your class gives it. That means adding a field later just works: old saves read, and the new field starts as you set it.

If several values point to the same object, they still point to one object after reading. A list that points back at the object holding it reads back the same way.

## Objs in a save

An Obj is found again by the scene it was loaded from, so read a save after its level is loaded. If the scene isn't open, the Obj reads as `Obj.None`, and the Console says so.

An Obj made while the game runs, such as a spawned enemy, isn't in any scene file, so `Save.Write` can't find it again. A [save of the scene](#saving-the-scene-as-the-player-left-it) makes such Objs again for you.

## Saving a behaviour's fields

A save can't make a behaviour, because a behaviour belongs on an Obj. Instead, write the behaviour and read the save back into it:

```csharp
public sealed class Player : Behaviour
{
	public int Coins;
	[SerializeField] private float stamina = 1;

	public void SaveMe() => Save.Write("player", this);

	public void LoadMe() => Save.ReadInto("player", this);
}
```

`ReadInto` works on any object that already exists, and fields that aren't in the save keep their current values.

## A menu of saves

`Save.Slots` lists every save, newest first, for a load menu:

```csharp
foreach (var slot in Save.Slots)
{
	Log.Info($"{slot.Name}: {slot.Summary}, saved {slot.Time:g}");
}
```

The summary is anything you pass when saving, such as the chapter and play time:

```csharp
Save.Write("slot1", data, $"Chapter {data.Chapter}, {played.Hours}h {played.Minutes}m");
```

`Save.Slots` reads only the start of each file, so a menu with many saves opens quickly. Even so, keep the list while the menu is open rather than asking for it every frame.

You can also look after slots from code:

```csharp
if (Save.Exists("slot1"))
{
	Save.Copy("slot1", "slot1 backup");
}
Save.Rename("slot2", "Hardcore run");
Save.Delete("slot3");
```

Slot names can use letters, digits, spaces, `-` and `_`. Capital letters don't count: `Slot1` and `slot1` are the same save on every computer, as they are on Windows.

## Saving without a pause

`Save.Write` waits until the save is safely on disk. That's usually too quick to notice, but on a slow disk it can show as a hitch. `WriteAsync` writes on another thread while the game carries on:

```csharp
private async void Autosave()
{
	try
	{
		await Save.WriteAsync("autosave", data);
		Log.Info("Saved");
	}
	catch (SaveException failure)
	{
		Log.Error(failure.Message);
	}
}
```

The values are copied when you call `WriteAsync`, so the game can change them straight away. Code after the `await` runs at the start of a frame once the file is written.

## Several parts in one save

A `SaveFile` holds several values under names of their own, written together. That lets each part of your game save its own values without one class holding everything:

```csharp
var file = new SaveFile { Summary = "The caves" };
file.Set("player", player);
file.Set("world", world.State);
file.Set("quests", quests);
await file.WriteAsync("slot1");
```

```csharp
var file = SaveFile.Read("slot1");
file.ReadInto("player", player);
world.State = file.Get<WorldState>("world");
var quests = file.Get("quests", new QuestLog());   // a default if the save has none
```

## Saving the scene as the player left it

Some things in a level should stay as the player left them: a door they opened, a chest they emptied, coins they picked up, enemies that were spawned. Rather than writing a class that holds every door, mark the values to keep with `[Saved]`, and save the scene:

```csharp
public sealed class Door : Behaviour
{
	[Saved] public bool IsOpen;     // kept in a save of the scene
	public float OpenSpeed = 2;     // set in the scene file, so not saved

	protected override void Start()
	{
		if (IsOpen)
		{
			SnapOpen();
		}
	}
}

[Saved]   // on the class: every field this behaviour has is kept
public sealed class Chest : Behaviour
{
	public bool Looted;
	public List<string> Items = ["potion", "key"];
}
```

```csharp
// The Save button:
Save.WriteScene("slot1", "The caves, 1h 20m");

// Continue, on the main menu:
await Save.LoadScene("slot1");
```

`[Saved]` works on private fields and on properties too. A save of the scene keeps:

- every `[Saved]` value, on every behaviour in the open scenes
- which of the Objs loaded from scene files have been destroyed, so a coin the player picked up stays gone, with nothing to mark
- every Obj your scripts made from a prefab, with its parent, position, rotation and scale, and its own `[Saved]` values

`LoadScene` loads the saved level again, with all its scenes loaded fresh. Then it destroys the Objs that were destroyed, removes any Objs your scripts had spawned, makes the saved ones again, and sets every `[Saved]` value. Values that point at a spawned Obj, such as a spawner's list of its enemies, point at the new copies:

```csharp
public sealed class WolfSpawner : Behaviour
{
	[SerializeField] private Prefab wolf;
	[Saved] private List<Obj> wolves = [];

	public void Spawn(Vector3 at) => wolves.Add(wolf.Instantiate(at, Quaternion.Identity));
}
```

Behaviours in the reloaded scene get their saved values before their `Awake` and `Start` run, so those can act on them, as the door above does. For the same reason, a spawner that makes its first enemies in `Awake` or `Start` should skip that while a save is loading, because the save makes the ones it had again:

```csharp
protected override void Start()
{
	if (!Save.IsLoadingScene)
	{
		SpawnFirstWave();
	}
}
```

`Save.IsLoadingScene` is true from `LoadScene` until the end of the frame the save is put back in.

To keep where an Obj from a scene file is, such as a crate the player can push, add the `SavedTransform` behaviour to it. Objs your scripts spawn don't need it.

A save of the scene is a value like any other, so it can be one part of a `SaveFile` beside your own:

```csharp
var file = new SaveFile { Summary = "The caves" };
file.Set("scene", SceneState.Capture());
file.Set("player", playerData);
await file.WriteAsync("slot1");
```

```csharp
var read = SaveFile.Read("slot1");
await read.Get<SceneState>("scene").LoadAsync();
player.Data = read.Get<PlayerData>("player");
```

If the level is already freshly loaded, `state.Apply()` puts the state back without loading it again.

## Binary or JSON

Saves can be written in two formats:

- **Binary** is small and quick. Builds use it.
- **JSON** is text you can read and change. The editor and `vox run` use it, so you can open a save to see what's in it, or edit it to test something.

A save remembers which format it's in, so either one reads back anywhere. To choose a format yourself:

```csharp
Save.Format = SaveFormat.Binary;                          // for every save from now on
var file = new SaveFile { Format = SaveFormat.Json };    // for one save
```

`Save.Folder` gives the folder your saves are in.

## Changing what you save

As your game grows, the values it saves will change. Adding a field needs nothing. For anything else, such as renaming a field or changing its type, give the class a version and a **migration** that updates older saves:

```csharp
[SaveVersion(3)]
public sealed class GameData
{
	public int Health = 100;
	public List<string> Items = [];

	// Version 1 called health HP.
	[MigrateFrom(1)]
	private static void RenameHp(SaveNode data) => data.Rename("HP", "Health");

	// Version 2 kept the items in one string, split by commas.
	[MigrateFrom(2)]
	private static void SplitItems(SaveNode data) =>
		data["Items"] = SaveNode.From(data["Items"].AsString()!.Split(','));
}
```

A class without `[SaveVersion]` is version 1. When a save is read, migrations run one after another, so a save from version 1 goes through the migrations for 1 and then 2. A version with nothing to change doesn't need one.

A migration gets the saved values as a `SaveNode`: a table of names and values that you can read, change, rename and remove before the save is read into your class. `data.ToString()` shows it as JSON, which helps while you're writing one.

If a newer version of your game wrote the save, reading it throws a `SaveException` whose `Error` is `SaveError.NewerVersion`, rather than guessing and losing what the newer version saved.

## When a save can't be read

`Save.Read` throws a `SaveException` when it can't read a save. Its `Error` says why:

| `Error` | Why |
| --- | --- |
| `Missing` | There's no save with that name |
| `Damaged` | The file is cut short, isn't a save, or was changed after it was written |
| `NewerVersion` | A newer version of the game wrote it |
| `Failed` | The disk refused, for example because it's full |
| `BadName` | The name can't be used for a file |

For settings and other saves that may not exist yet, give a default instead:

```csharp
var settings = Save.Read("settings", new Settings());

if (Save.TryRead<GameData>("slot1", out var data))
{
	Continue(data);
}
```

Both of these give the default, or `false`, when there's no save, and also when the save can't be read, with the reason in the Console.

A single value that no longer fits, such as text where a number is expected, doesn't stop the rest of the save from reading. That value starts as your class sets it, and the Console says which value it was.

## Saving a type your own way

To save a type differently from its fields, for example a type from a library whose fields are private, write a **converter**. Your game finds it by itself:

```csharp
public sealed class MoneyConverter : SaveConverter<Money>
{
	public override SaveNode Write(Money money) => money.Cents;

	public override Money Read(SaveNode node) => new(node.AsLong());
}
```

Inside a converter, `SaveNode.From(value)` and `node.As<T>()` save and read the values your type holds, the same way the rest of the save does.

## Adding a format

A format turns a save's values into bytes and back. To add one, such as a format that compresses another, derive from `SaveFormat`. Your game finds it by itself, and saves written with it record its `Name`, so they're read back with it:

```csharp
using System.IO.Compression;

public sealed class CompressedFormat : SaveFormat
{
	public override string Name => "binary-brotli";

	public override byte[] Write(SaveNode root)
	{
		using var packed = new MemoryStream();
		using (var brotli = new BrotliStream(packed, CompressionLevel.Optimal))
		{
			brotli.Write(Binary.Write(root));
		}
		return packed.ToArray();
	}

	public override SaveNode Read(ReadOnlySpan<byte> bytes)
	{
		using var brotli = new BrotliStream(new MemoryStream(bytes.ToArray()), CompressionMode.Decompress);
		using var unpacked = new MemoryStream();
		brotli.CopyTo(unpacked);
		return Binary.Read(unpacked.ToArray());
	}
}
```

## Where saves are kept

Each game has its own save folder in the player's data folder, separate from where the game is installed:

| | Folder |
| --- | --- |
| Linux | `~/.local/share/<company>/<game>/Saves` |
| Windows | `%LOCALAPPDATA%\<company>\<game>\Saves` |

The company and game names come from the build profile. In the editor and with `vox run`, saves go to the folder named after the project, with no company. A build whose profile leaves both names unset uses that same folder.
