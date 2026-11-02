using System.Linq;
using Voxear.Saving;

namespace Voxear.Tests;

/// <summary>Saves of the scene away from the engine: which values [Saved] marks, and the keys of spawned Objs.</summary>
internal static class SceneStateTests
{
	[Test]
	private static void SavedMarksFieldsAndPropertiesPrivateOnesToo()
	{
		var names = SceneMembers.Of(typeof(TestDoor)).Select(member => member.Name).ToList();
		Check.True(names.SequenceEqual(new[] { "IsOpen", "knocks", "Swing" }), string.Join(", ", names));

		var door = new TestDoor { IsOpen = true };
		door.Knock(0.5f);
		var values = SaveWriter.WriteMembers(door, SceneMembers.Of(typeof(TestDoor)));
		Check.True(!values.Has("Width") && !values.Has("Hinges"), $"only the marked ones, and only what can be set: {values}");

		var fresh = new TestDoor();
		SaveReader.ReadMembers(values, fresh, SceneMembers.Of(typeof(TestDoor)));
		Check.True(fresh.IsOpen && fresh.Knocks == 1 && fresh.SwingNow == 0.5f, "put back");
	}

	[Test]
	private static void SavedOnAClassKeepsWhatASaveKeepsOfIt()
	{
		var names = SceneMembers.Of(typeof(TestChest)).Select(member => member.Name).ToList();
		Check.True(names.SequenceEqual(new[] { "Looted", "Items" }), string.Join(", ", names));
		Check.True(SceneMembers.Of(typeof(TestSpawner)).Count == 0, "a behaviour with nothing marked keeps nothing");
	}

	[Test]
	private static void SpawnedObjsAreNumberedInASaveOfTheScene()
	{
		Check.True(SpawnedKeys.ForSave("spawned:42/0") == null, "outside a save of the scene, a copy has no key to keep");
		Check.True(SpawnedKeys.ForSave("0123:5") == "0123:5", "an Obj in a scene keeps its own key");
		using (SpawnedKeys.Writing(new() { [42] = 0, [77] = 1 }))
		{
			Check.True(SpawnedKeys.ForSave("spawned:77/3") == "spawned:1/3", "numbered by the save");
			Check.True(SpawnedKeys.ForSave("spawned:99/0") == null, "a copy the save does not keep");
		}

		var made = new[] { new Obj(5ul << 32 | 9), Obj.None };
		using (SpawnedKeys.Reading(made))
		{
			Check.True(SpawnedKeys.ForLookup("spawned:0/2") == $"spawned:{5ul << 32 | 9}/2", "the copy made again");
			Check.True(SpawnedKeys.ForLookup("spawned:1/0") == null && SpawnedKeys.ForLookup("spawned:7/0") == null, "one not made again");
		}
	}

	[Test]
	private static void ASceneStateSavesLikeAnyValue()
	{
		var file = new SaveFile();
		file.Set("scene", SceneState.Capture());
		file.Set("note", "kept beside it");
		file.Write("Scene State");
		var read = SaveFile.Read("Scene State");
		var state = read.Get<SceneState>("scene");
		Check.True(state.Root["saved"].Kind == SaveNodeKind.List && read.Get<string>("note") == "kept beside it", read.Root.ToString());
		state.Apply();
	}
}
