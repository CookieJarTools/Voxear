using System.Linq;

namespace Voxear.Tests;

/// <summary>The pieces of controls screens with no engine behind them: no actions, nothing bound, and nothing throws.</summary>
internal static class ControlsScreenTests
{
	[Test]
	private static void TheControlsPiecesAreEmptyWithoutTheEngine()
	{
		Check.True(Input.Actions.Count == 0 && Input.Categories.Count == 0, "no actions or categories");
		Check.True(Input.ControlUses(Key.Space).Count == 0, "nothing bound");
		InputControl space = Key.Space;
		Check.True(space.Label == "Keyboard/Space" && space.Path == "Keyboard/Space", "a control is named by its path");
		Check.True(InputControl.Keyboard(Key.Digit7).Path == "Keyboard/7", "a digit by itself");
		Check.True(InputControl.Gamepad(GamepadAxis.LeftStickUp).Device == ControlDevice.Gamepad, "a stick's half");
		Check.True(space == InputControl.Keyboard(Key.Space) && space != (InputControl)MouseButton.Left, "the same control is equal");
		Check.True(InputControl.FromPath("Mouse/Delta") == MouseAxis.Delta && InputControl.FromPath("") == null, "read back");
		var jump = Input.Action("Jump");
		Check.True(jump.GetBindings().Count == 0 && jump.Category == null && !jump.Hidden, "an action that is not there");
		Check.True(Input.Conflicts(jump, Key.Space).Count == 0, "and no conflicts");
		Check.True(jump.Map.Name.Length == 0 && Input.Maps.Count == 0, "in no map");
		Check.True(Input.Part("up") == BindingPart.Up && Input.Part("") == null, "a part by its name");
		var list = new ControlsList { ShowHidden = true };
		Check.True(list.Children.Count(child => child.HasClass("controls-row")) == 1, "only the header row");
		var map = new KeymapView(ControlScheme.Gamepad) { Colors = KeymapColors.Categories, Map = Input.Map("Player") };
		Check.True(map.Scheme == ControlScheme.Gamepad && map.Map == Input.Map("Player"), "a key map keeps what it was set");
		var prompt = new PromptView("Jump") { Action = "Player/Crouch" };
		Check.True(prompt.Action == "Player/Crouch", "a prompt keeps its action");
		Check.True(!Input.Joining && Input.Players.Count == 0, "nobody has joined");
		Input.Joining = true;
		Input.SetMostPlayers(2);
	}
}
