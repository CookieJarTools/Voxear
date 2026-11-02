using System.Linq;

namespace Voxear.Tests;

/// <summary>The pieces of controls screens with no engine behind them: no actions, nothing bound, and nothing throws.</summary>
internal static class ControlsScreenTests
{
	[Test]
	private static void TheControlsPiecesAreEmptyWithoutTheEngine()
	{
		Check.True(Input.Actions.Count == 0 && Input.Categories.Count == 0, "no actions or categories");
		Check.True(Input.ControlUses("Keyboard/Space").Count == 0, "nothing bound");
		Check.True(Input.ControlLabel("Keyboard/Space") == "Keyboard/Space", "a control is its path");
		var jump = Input.Action("Jump");
		Check.True(jump.GetBindings().Count == 0 && jump.Category == null && !jump.Hidden, "an action that is not there");
		Check.True(Input.Conflicts(jump, "Keyboard/Space").Count == 0, "and no conflicts");
		var list = new ControlsList { ShowHidden = true };
		Check.True(list.Children.Count(child => child.HasClass("controls-row")) == 1, "only the header row");
		var map = new KeymapView(ControlScheme.Gamepad) { Colors = KeymapColors.Categories, Map = "Player" };
		Check.True(map.Scheme == ControlScheme.Gamepad && map.Map == "Player", "a key map keeps what it was set");
	}
}
