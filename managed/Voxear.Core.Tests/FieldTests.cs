using System.Linq;
using System.Numerics;
using Voxear.Scripting;

namespace Voxear.Tests
{
	internal static class FieldTests
	{
		[Test]
		private static void NestedSerializableTypesFillFromTables()
		{
			var spawner = new TestSpawner();
			const string json = """
				{
					"settings": { "offset": [1, 2, 3], "waves": [{ "count": 2, "delay": 0.5 }, { "count": 3 }] },
					"extra": [{ "count": 9 }],
					"target": { "entity": 4294967297 }
				}
				""";
			BehaviourFields.Apply(spawner, json);

			Check.Near(new Vector3(1f, 2f, 3f), spawner.Settings.Offset);
			Check.True(spawner.Settings.Waves.Length == 2, "two waves");
			Check.True(spawner.Settings.Waves[0].Count == 2 && spawner.Settings.Waves[1].Count == 3, "wave counts");
			Check.Near(0.5f, spawner.Settings.Waves[0].Delay);
			Check.True(spawner.Extra.Count == 1 && spawner.Extra[0].Count == 9, "a list of structs");
			Check.True(spawner.Target.IsValid, "an Obj reference");
		}

		[Test]
		private static void ColoursFillFromHexOrNumbers()
		{
			var lamp = new TestLamp();
			BehaviourFields.Apply(lamp, """{ "tint": "#00ff0080", "glow": [0.25, 0.5, 1], "shade": [1, 0, 0, 0.5] }""");
			Check.True(lamp.Tint == new Color(0f, 1f, 0f, 128 / 255f), "hex, with alpha");
			Check.True(lamp.Glow == new Color(0.25f, 0.5f, 1f), "three numbers are opaque");
			Check.True(lamp.Shade == new Color(1f, 0f, 0f, 0.5f), "four numbers, alpha last");
		}

		[Test]
		private static void LevelAndSceneFieldsAreReferencesToTheirFiles()
		{
			var traveller = new TestTraveller();
			BehaviourFields.Apply(traveller, """{ "next": "Levels/Night.level", "room": "Scenes/Cellar.scene" }""");
			Check.True(traveller.Next.IsSet && traveller.Next.Reference == "Levels/Night.level", "the level, as written");
			Check.True(traveller.Room == SceneAsset.Load("Scenes/Cellar.scene"), "the scene, as written");
			Check.True(!new TestTraveller().Next.IsSet, "an unset field names no level");

			using var document = System.Text.Json.JsonDocument.Parse(ScriptCatalog.Describe(typeof(TestTraveller).Assembly));
			var behaviour = document.RootElement.GetProperty("behaviours").EnumerateArray()
				.Single(b => b.GetProperty("name").GetString() == "Voxear.Tests.TestTraveller");
			var next = behaviour.GetProperty("fields").EnumerateArray().Single(f => f.GetProperty("name").GetString() == "Next");
			Check.True(next.GetProperty("type").GetProperty("kind").GetString() == "asset", "described as an asset field");
			Check.True(next.GetProperty("type").GetProperty("asset").GetString() == "Voxear.LevelAsset", "of levels");
		}

		[Test]
		private static void AComponentFieldTakesAnObjWithTheComponent()
		{
			var radio = new TestRadio();
			BehaviourFields.Apply(radio, """{ "speaker": {} }""");
			Check.True(radio.Speaker.Equals(AudioSource.None), "an Obj not saved with the scene reads as none");

			using var document = System.Text.Json.JsonDocument.Parse(ScriptCatalog.Describe(typeof(TestRadio).Assembly));
			var behaviour = document.RootElement.GetProperty("behaviours").EnumerateArray()
				.Single(b => b.GetProperty("name").GetString() == "Voxear.Tests.TestRadio");
			var speaker = behaviour.GetProperty("fields").EnumerateArray().Single(f => f.GetProperty("name").GetString() == "Speaker");
			Check.True(speaker.GetProperty("type").GetProperty("kind").GetString() == "component", "described as a component field");
			Check.True(speaker.GetProperty("type").GetProperty("name").GetString() == "Voxear.AudioSource", "of Audio Sources");
		}

		[Test]
		private static void BadNestedValuesKeepTheirDefaultsAndTheRestStillLoads()
		{
			var spawner = new TestSpawner();
			BehaviourFields.Apply(spawner, """{ "settings": { "waves": [{ "count": "many" }], "colour": 1 }, "lives": 5 }""");
			Check.True(spawner.Settings.Waves.Length == 1 && spawner.Settings.Waves[0].Count == 0, "the bad count stays 0");
			Check.True(spawner.Lives == 5, "fields after a bad one still load");
		}
	}
}
