using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using Voxear.Scripting;

namespace Voxear.Tests
{
	internal static class DrawerTests
	{
		[Test]
		private static void TheCatalogNamesEachFieldsDrawersOutermostFirst()
		{
			using var document = JsonDocument.Parse(ScriptCatalog.Describe(typeof(TestHealthBar).Assembly));
			var bar = document.RootElement.GetProperty("behaviours").EnumerateArray().Single(b => b.GetProperty("name").GetString() == "Voxear.Tests.TestHealthBar");
			var fields = bar.GetProperty("fields").EnumerateArray().ToDictionary(f => f.GetProperty("name").GetString()!);

			Check.True(Names(fields["Health"], "drawers").SequenceEqual(new[] { "Voxear.Tests.TestHealthDrawer" }), "a type's drawer draws its fields");
			var shield = Names(fields["Shield"], "drawers");
			Check.True(shield.SequenceEqual(new[] { "Voxear.Tests.TestTagDrawer", "Voxear.Tests.TestHealthDrawer" }), string.Join(", ", shield));
			Check.True(Names(fields["Roll"], "drawers").SequenceEqual(new[] { "Voxear.Tests.TestStepDrawer" }), "an attribute's drawer");
			Check.True(Names(fields["Name"], "drawers").Count == 0, "an attribute drawer only draws the values it is for");
			Check.True(Names(fields["Extras"], "drawers").Count == 0, "a list is not drawn whole");
			Check.True(Names(fields["Extras"], "item_drawers").SequenceEqual(new[] { "Voxear.Tests.TestHealthDrawer" }), "but each of its items is");
		}

		[Test]
		private static void DrawersAreOpenedWithTheirAttributesAndDrawnInnermostFirst()
		{
			var opened = Ask(new
			{
				action = "open",
				fields = new object[]
				{
					new { type = "Voxear.Tests.TestHealthBar", path = new object[] { "shield" }, drawers = new[] { "Voxear.Tests.TestTagDrawer", "Voxear.Tests.TestHealthDrawer" }, label = "Shield" },
					new { type = "Voxear.Tests.TestHealthBar", path = new object[] { "Roll" }, drawers = new[] { "Voxear.Tests.TestStepDrawer" }, label = "Roll" },
					new { type = "Voxear.Tests.TestHealthBar", path = new object[] { "Extras", 0 }, drawers = new[] { "Voxear.Tests.TestHealthDrawer" }, label = "0" },
					new { type = "Voxear.Tests.TestHealthBar", path = new object[] { "Missing" }, drawers = new[] { "Voxear.Tests.TestStepDrawer" }, label = "Missing" },
				},
			});
			var markups = opened.GetProperty("fields")[0].GetProperty("markups").EnumerateArray().Select(m => m.GetString()).ToList();
			Check.True(markups.Count == 2 && markups[0]!.Contains("next") && markups[1]!.Contains("text"), string.Join(" | ", markups));
			Check.True(opened.GetProperty("fields")[3].TryGetProperty("error", out _), "a field that is not there cannot be drawn");

			TestHealthDrawer.Drawn.Clear();
			var drawn = Ask(new
			{
				action = "draw",
				fields = new object[]
				{
					new { field = 0, documents = new[] { 1, 2 }, value = new { Current = 1, Max = 5 } },
					new { field = 1, documents = new[] { 3 }, value = 3 },
					new { field = 1, documents = new int[0], value = 3 },
					new { field = 2, documents = new[] { 4 }, value = new { Current = 5, Max = 5 } },
					new { field = 3, documents = new[] { 5 }, value = 1 },
				},
			}).GetProperty("fields");
			Check.True(drawn[0].GetProperty("value").GetProperty("Current").GetInt32() == 2, drawn[0].GetRawText());
			Check.True(TestHealthDrawer.Drawn.Take(2).SequenceEqual(new[] { "health", "tag" }), string.Join(", ", TestHealthDrawer.Drawn));
			Check.True(drawn[1].GetProperty("value").GetInt32() == 5, "the attribute's step was added: " + drawn[1].GetRawText());
			Check.True(drawn[1].GetProperty("changed").GetBoolean(), "and it says so");
			Check.True(drawn[2].GetProperty("value").GetInt32() == 3, "a drawer whose markup was not built does not draw");
			Check.True(!drawn[2].GetProperty("changed").GetBoolean(), "so nothing changed");
			Check.True(drawn[3].GetProperty("value").GetProperty("Current").GetInt32() == 5, "a list's item is drawn as its type is");
			Check.True(drawn[4].TryGetProperty("error", out _), "a field that was not opened is not drawn");
		}

		private static List<string> Names(JsonElement field, string name)
		{
			if (!field.TryGetProperty("inspector", out var inspector) || !inspector.TryGetProperty(name, out var drawers))
			{
				return new List<string>();
			}
			return drawers.EnumerateArray().Select(d => d.GetString()!).ToList();
		}

		private static JsonElement Ask(object request)
		{
			var answer = InspectorCalls.Answer(JsonSerializer.Serialize(request));
			using var document = JsonDocument.Parse(answer);
			return document.RootElement.Clone();
		}
	}
}
