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
		private static void BadNestedValuesKeepTheirDefaultsAndTheRestStillLoads()
		{
			var spawner = new TestSpawner();
			BehaviourFields.Apply(spawner, """{ "settings": { "waves": [{ "count": "many" }], "colour": 1 }, "lives": 5 }""");
			Check.True(spawner.Settings.Waves.Length == 1 && spawner.Settings.Waves[0].Count == 0, "the bad count stays 0");
			Check.True(spawner.Lives == 5, "fields after a bad one still load");
		}
	}
}
