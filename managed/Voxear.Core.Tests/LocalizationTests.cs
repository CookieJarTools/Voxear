using Voxear.Scripting;

namespace Voxear.Tests;

/// <summary>Localization with no engine behind it: a key is its own text, in English, and nothing throws.</summary>
internal static class LocalizationTests
{
	[Test]
	private static void AKeyIsItsOwnTextWithoutTheEngine()
	{
		Check.True(Localization.Get("menu.play") == "menu.play", "the key");
		Check.True(Localization.Format("loot.found", ("count", 3)) == "loot.found", "with values too");
		var changed = false;
		Localization.Changed += () => changed = true;
		Localization.Language = "fr";
		Check.True(Localization.Language == "en" && !changed, "English, unchanged");
		Check.True(Localization.Languages.Count == 1 && Localization.Languages[0] == "en", "only English");
	}

	[Test]
	private static void ALocalizedStringFieldReadsItsKeyFromTheScene()
	{
		var sign = new TestSign();
		Check.True(sign.Title.Key == "sign.welcome" && sign.Title.Value == "sign.welcome", "the key, its own text here");
		Check.True(!sign.Subtitle.IsSet && sign.Subtitle.Value == string.Empty && sign.Subtitle == default, "none");
		BehaviourFields.Apply(sign, """{ "title": "sign.goodbye", "subtitle": "sign.small" }""");
		Check.True(sign.Title == "sign.goodbye" && sign.Subtitle.Key == "sign.small", "as the scene writes them");
		Check.True($"{sign.Title}" == "sign.goodbye", "shown as its text");
	}
}
