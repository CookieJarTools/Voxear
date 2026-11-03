using System.Collections.Generic;
using System.Linq;

namespace Voxear.Tests;

internal static class UiCallbackTests
{
	[Test]
	private static void HandlersGivenAsElementsAreMadeRunWhenTheyAreUsed()
	{
		var heard = new List<string>();
		var root = new Panel();
		var fill = root.Add(new Button("Fill", () => heard.Add("fill")));
		var reset = root.Add(new Button("Reset").OnClicked(() => heard.Add("reset")));
		var snap = root.Add(new Toggle("Snap").OnToggled(on => heard.Add($"snap {on}")));
		var size = root.Add(new Slider(0f, 10f).OnMoved(value => heard.Add($"size {value}")));
		var name = root.Add(new TextField().OnTyped(text => heard.Add($"name '{text}'")));
		var mode = root.Add(new Dropdown("Walk", "Run").OnPicked(index => heard.Add($"mode {index}")));
		var any = root.Add(new Panel().OnChanged(() => heard.Add("changed")));
		Check.True(reset is Button && any is Panel, "each gives back the element as its own kind, to chain inside Add");

		fill.PretendClicked();
		reset.PretendClicked();
		snap.PretendChanged(1f);
		size.PretendChanged(4f);
		name.PretendChanged(0f);
		mode.PretendChanged(1f);
		any.PretendChanged(0f);
		root.DispatchTree();

		var expected = new[] { "fill", "reset", "snap True", "size 4", "name ''", "mode 1", "changed" };
		Check.True(heard.Count == expected.Length && heard.TrueForAll(expected.Contains), string.Join(", ", heard));
	}

	[Test]
	private static void AnElementsOwnEventIsHeardWithoutAnyOtherHandler()
	{
		var picked = -1;
		var mode = new Dropdown("Walk", "Run");
		mode.Picked += index => picked = index;
		Check.True(mode.HasListeners, "Picked alone is listened for");
		mode.PretendChanged(1f);
		mode.DispatchTree();
		Check.True(picked == 1, $"picked {picked}");
	}

	[Test]
	private static void BoundTextIsWorkedOutAgainUntilTextIsSet()
	{
		var count = 1;
		var label = new Label().Bind(() => $"{count} coins");
		Check.True(label.Text == "1 coins", label.Text);
		count = 2;
		label.RefreshBound(0);
		Check.True(label.Text == "2 coins", "worked out again: " + label.Text);
		label.Text = "None";
		count = 3;
		label.RefreshBound(0);
		Check.True(label.Text == "None", "setting Text stops the binding: " + label.Text);
	}

	[Test]
	private static void BoundValuesSetSlidersTogglesAndDropdowns()
	{
		var (health, on, pick) = (3f, true, 1);
		var bar = new Slider(0f, 10f).Bind(() => health);
		var snap = new Toggle("Snap").Bind(() => on);
		var mode = new Dropdown("Walk", "Run").Bind(() => pick);
		Check.True(bar.Value == 3f && snap.Checked && mode.Selected == 1, "bound as they are made");
		(health, on, pick) = (7f, false, 0);
		foreach (var element in new UiElement[] { bar, snap, mode })
		{
			element.RefreshBound(0);
		}
		Check.True(bar.Value == 7f && !snap.Checked && mode.Selected == 0, "and again as the host refreshes");
	}

	[Test]
	private static void ElementsShowKeysTextAndFollowIt()
	{
		// Without the engine, a key's text is the key itself.
		Check.True(new Label(new LocalizedString("menu.play")).Text == "menu.play", "a label made with a key");
		Check.True(new Button(new LocalizedString("menu.quit"), () => { }).Text == "menu.quit", "a button with a handler");
		Check.True(new Label().Localize(new LocalizedString("hud.coins"), ("count", 3)).Text == "hud.coins", "Localize");
		var title = new LocalizedString("menu.title");
		Check.True(new Label().Bind(() => title).Text == "menu.title", "a key that can change");
		var easy = new LocalizedString("difficulty.easy");
		var mode = new Dropdown(easy, "Custom");
		Check.True(mode.Options[0] == "difficulty.easy" && mode.Options[1] == "Custom", string.Join(", ", mode.Options));

		var hint = new Button("Fill") { Tooltip = new LocalizedString("hints.fill") };
		Check.True(hint.Tooltip.IsLocalized && hint.Tooltip.ToString() == "hints.fill", "a tooltip from a key");
		var name = new TextField { Placeholder = "Your name" };
		Check.True(!name.Placeholder.IsLocalized && name.Placeholder.ToString() == "Your name", "a placeholder written out");
		Check.True(new UiText("@home").ToString() == "@home", "text written out is shown as it is");
	}

	[Test]
	private static void ANextFieldIsMarkedForTheEditorToFind()
	{
		Check.True(new NextField().HasClass("field-drawer-next"), "the editor puts what comes next in it");
	}
}
