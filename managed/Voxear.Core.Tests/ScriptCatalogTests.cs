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
		private static void CommandsThatTakeAndGiveNothingAreListed()
		{
			using var document = JsonDocument.Parse(ScriptCatalog.Describe(typeof(TestCommands).Assembly));
			var commands = document.RootElement.GetProperty("commands").EnumerateArray().ToList();
			Check.True(commands.Count == 1, $"one command, not {commands.Count}");
			var tidy = commands[0];
			Check.True(tidy.GetProperty("title").GetString() == "Tidy Up" && tidy.GetProperty("keys").GetString() == "Ctrl+Shift+T", "its title and keys");
			Check.True(tidy.GetProperty("type").GetString() == "Voxear.Tests.TestCommands" && tidy.GetProperty("method").GetString() == "TidyUp", "and where it is");
		}

		private static string? Kind(JsonElement field)
		{
			return field.GetProperty("type").GetProperty("kind").GetString();
		}
	}
}
