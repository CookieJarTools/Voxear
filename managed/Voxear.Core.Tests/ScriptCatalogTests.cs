using System.Linq;
using System.Text.Json;
using Voxear.Scripting;

namespace Voxear.Tests
{
	internal static class ScriptCatalogTests
	{
		[Test]
		private static void BehavioursAreDescribedWithTheirFieldsKindsAndStartingValues()
		{
			using var document = JsonDocument.Parse(ScriptCatalog.Describe(typeof(TestSpawner).Assembly));
			var behaviours = document.RootElement.GetProperty("behaviours").EnumerateArray().ToList();
			var spawner = behaviours.Single(b => b.GetProperty("name").GetString() == "Voxear.Tests.TestSpawner");
			var fields = spawner.GetProperty("fields").EnumerateArray().ToDictionary(f => f.GetProperty("name").GetString()!);

			Check.True(fields.Keys.SequenceEqual(new[] { "Settings", "Extra", "Target", "Lives" }), string.Join(", ", fields.Keys));
			Check.True(Kind(fields["Lives"]) == "whole" && fields["Lives"].GetProperty("default").GetInt64() == 3, "Lives starts at 3");
			Check.True(Kind(fields["Target"]) == "obj", "an Obj reference");
			var extra = fields["Extra"].GetProperty("type");
			Check.True(Kind(fields["Extra"]) == "list" && extra.GetProperty("element").GetProperty("name").GetString() == "Voxear.Tests.Wave", "a list of Waves");
			Check.True(fields["Settings"].GetProperty("default").GetProperty("Waves").GetArrayLength() == 0, "a nested table starts empty");

			var wave = document.RootElement.GetProperty("types").GetProperty("Voxear.Tests.Wave");
			var waveFields = wave.GetProperty("fields").EnumerateArray().Select(f => f.GetProperty("name").GetString()).ToList();
			Check.True(waveFields.SequenceEqual(new[] { "Count", "delay" }), "nested types are described once, with private [SerializeField] fields");
		}

		[Test]
		private static void ComponentsAreDescribedAsBehavioursAreButApart()
		{
			using var document = JsonDocument.Parse(ScriptCatalog.Describe(typeof(TestDrift).Assembly));
			var components = document.RootElement.GetProperty("components").EnumerateArray().ToList();
			var names = components.Select(c => c.GetProperty("name").GetString()).ToList();
			Check.True(names.Contains("Voxear.Tests.TestDrift"), string.Join(", ", names));
			Check.True(!names.Contains("Voxear.Tests.TestHolder"), "a struct holding a reference is not a component");
			var behaviours = document.RootElement.GetProperty("behaviours").EnumerateArray().Select(b => b.GetProperty("name").GetString());
			Check.True(!behaviours.Contains("Voxear.Tests.TestDrift"), "components are not behaviours");

			var drift = components.Single(c => c.GetProperty("name").GetString() == "Voxear.Tests.TestDrift");
			var fields = drift.GetProperty("fields").EnumerateArray().ToDictionary(f => f.GetProperty("name").GetString()!);
			Check.True(fields.Keys.SequenceEqual(new[] { "Velocity", "Wobble", "Anchor" }), string.Join(", ", fields.Keys));
			Check.True(Kind(fields["Velocity"]) == "vector" && Kind(fields["Anchor"]) == "obj", "vectors and Obj references");
			Check.True(fields["Wobble"].GetProperty("inspector").GetProperty("range")[1].GetDouble() == 10, "with their attributes");
			Check.True(fields["Wobble"].GetProperty("default").GetDouble() == 0, "starting at zero");

			// Where each lies in the struct's bytes: a Vector3, a float, then an Obj aligned to eight.
			var offsets = fields.Values.Select(f => f.GetProperty("layout").GetProperty("offset").GetInt32()).ToList();
			Check.True(offsets.SequenceEqual(new[] { 0, 12, 16 }), string.Join(", ", offsets));
			Check.True(fields["Anchor"].GetProperty("layout").GetProperty("size").GetInt32() == 8, "an Obj is eight bytes");
		}

		[Test]
		private static void CommandsThatTakeAndGiveNothingAreListed()
		{
			using var document = JsonDocument.Parse(ScriptCatalog.Describe(typeof(TestCommands).Assembly));
			var commands = document.RootElement.GetProperty("commands").EnumerateArray().ToList();
			Check.True(commands.Count == 1, $"one command, not {commands.Count}");
			var tidy = commands[0];
			Check.True(tidy.GetProperty("title").GetString() == "Tidy Up" && tidy.GetProperty("keys").GetString() == "Ctrl+Shift+T", "its title and keys");
			Check.True(tidy.GetProperty("type").GetString() == "Voxear.Tests.TestCommands" && tidy.GetProperty("method").GetString() == "TidyUp", "and where it is");
		}

		[Test]
		private static void InspectorAttributesAreWrittenWithTheirFields()
		{
			using var document = JsonDocument.Parse(ScriptCatalog.Describe(typeof(TestMover).Assembly));
			var mover = document.RootElement.GetProperty("behaviours").EnumerateArray()
				.Single(b => b.GetProperty("name").GetString() == "Voxear.Tests.TestMover");
			var fields = mover.GetProperty("fields").EnumerateArray().ToDictionary(f => f.GetProperty("name").GetString()!);

			var speed = fields["speed"].GetProperty("inspector");
			var group = speed.GetProperty("group");
			Check.True(group.GetProperty("kind").GetString() == "foldout" && group.GetProperty("name").GetString() == "Movement", "its group");
			Check.True(speed.GetProperty("range")[1].GetDouble() == 20 && speed.GetProperty("suffix").GetString() == "m/s", "its range and unit");
			Check.True(speed.GetProperty("tooltip").GetString() == "How fast it walks.", "its tooltip");
			Check.True(fields["mode"].GetProperty("inspector").GetProperty("enum_buttons").GetBoolean(), "buttons for an enum");
			var lift = fields["liftForce"].GetProperty("inspector");
			var showIf = lift.GetProperty("show_if");
			Check.True(showIf.GetProperty("member").GetString() == "mode" && showIf.GetProperty("value").GetString() == "Fly", "a condition, its value as a scene writes it");
			Check.True(lift.GetProperty("min").GetDouble() == 0, "a least value");
			var hit = fields["hitEffect"].GetProperty("inspector");
			Check.True(hit.GetProperty("required").GetString() == string.Empty && hit.GetProperty("info").GetArrayLength() == 2, "required, with two info boxes");
			Check.True(hit.GetProperty("info")[1].GetProperty("type").GetString() == "warning", "the second a warning");
			Check.True(fields["secret"].GetProperty("inspector").GetProperty("hidden").GetBoolean(), "hidden from the Inspector");
			var title = fields["title"].GetProperty("inspector");
			Check.True(title.GetProperty("order").GetInt32() == -1 && title.GetProperty("label").GetString() == "Display name", "its order and label");
			Check.True(!fields["mode"].TryGetProperty("default", out var mode) || mode.GetString() == "Walk", "an enum starts at its first value");
		}

		private static string? Kind(JsonElement field)
		{
			return field.GetProperty("type").GetProperty("kind").GetString();
		}
	}
}
