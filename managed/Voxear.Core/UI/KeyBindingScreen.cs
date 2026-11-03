using System;

namespace Voxear;

/// <summary>
/// A ready-made controls screen for players, with two views, as tabs or one alone: the Layout, the keyboard and
/// mouse or a gamepad drawn with each bound key lit and labelled with its actions, a legend, and the actions of the
/// key clicked, each with a button to rebind it; and the List, a row for each action with its bindings, as most
/// games' settings show them (see ControlsList). Open it from a pause menu with <c>KeyBindingScreen.Open()</c>; it
/// closes with its Close button.
/// <code>
/// var screen = KeyBindingScreen.Open("Controls", ControlsView.Both);
/// screen.Rebound += (action, control) => Save.Write("controls", Input.SaveBindings());
/// </code>
/// <para>
/// It is built from the same pieces a screen of your own would be, so it can be changed rather than replaced: the
/// Keymap (a KeymapView), the List (a ControlsList), the Bar, Legend and Details panels, and the Document they are
/// in, whose sheets style its classes, <c>key-binding-screen</c> and those starting <c>kbs-</c>. Saving the player's
/// bindings is the game's to do, with Input.SaveBindings and Input.LoadBindings.
/// </para>
/// </summary>
public sealed class KeyBindingScreen
{
	private readonly Button layoutTab;
	private readonly Button listTab;
	private readonly Button keyboard;
	private readonly Button gamepad;
	private readonly Toggle categories;
	private readonly Toggle hidden;
	private readonly Panel layout;
	private readonly ScrollView listView;
	private readonly Label status;
	private InputControl? shown;

	private KeyBindingScreen(UiDocument document, string title, ControlsView views)
	{
		Document = document;
		var screen = document.Root.Add(new Panel("key-binding-screen"));
		Bar = screen.Add(new Panel("kbs-bar"));
		Bar.Add(new Label(title)).AddClass("kbs-title");
		layoutTab = Bar.Add(new Button("Layout"));
		listTab = Bar.Add(new Button("List"));
		keyboard = Bar.Add(new Button("Keyboard"));
		gamepad = Bar.Add(new Button("Gamepad"));
		Bar.Add(new Panel("kbs-spacer"));
		categories = Bar.Add(new Toggle("Categories"));
		hidden = Bar.Add(new Toggle("Hidden"));
		var reset = Bar.Add(new Button("Reset All"));
		var close = Bar.Add(new Button("Close"));
		layout = screen.Add(new Panel("kbs-layout"));
		Keymap = layout.Add(new KeymapView(Input.Scheme));
		Legend = layout.Add(new Panel("kbs-legend"));
		Details = layout.Add(new Panel("kbs-details"));
		listView = screen.Add(new ScrollView());
		listView.AddClass("kbs-list");
		List = listView.Add(new ControlsList());
		status = screen.Add(new Label(string.Empty));
		status.AddClass("kbs-status");

		foreach (var tab in new[] { layoutTab, listTab, keyboard, gamepad })
		{
			tab.AddClass("kbs-tab");
		}
		reset.AddClass("kbs-reset");
		close.AddClass("kbs-close");
		layoutTab.Visible = listTab.Visible = views == ControlsView.Both;
		layoutTab.Clicked += () => View = ControlsView.Layout;
		listTab.Clicked += () => View = ControlsView.List;
		keyboard.Clicked += () => Scheme = ControlScheme.KeyboardMouse;
		gamepad.Clicked += () => Scheme = ControlScheme.Gamepad;
		categories.Toggled += on => SetCategories(on);
		hidden.Toggled += on => List.ShowHidden = Keymap.ShowHidden = on;
		reset.Clicked += ResetAll;
		close.Clicked += Close;
		Keymap.KeyClicked += control => ShowControl(control);
		List.Rebound += (action, control) => Rebound?.Invoke(action, control);
		Scheme = Keymap.Scheme;
		View = views == ControlsView.List ? ControlsView.List : ControlsView.Layout;
		ShowLegend();
	}

	/// <summary>Runs once it has closed.</summary>
	public event Action? Closed;

	/// <summary>Runs with the action and the control it now reads, each time the player rebinds one.</summary>
	public event Action<InputAction, InputControl>? Rebound;

	public UiDocument Document { get; }

	/// <summary>The device drawn, with its actions.</summary>
	public KeymapView Keymap { get; }

	/// <summary>The bar along the top: the title, the device buttons, the toggles and Close.</summary>
	public Panel Bar { get; }

	/// <summary>What each colour means, under the device.</summary>
	public Panel Legend { get; }

	/// <summary>The clicked key's actions, each with its Rebind button.</summary>
	public Panel Details { get; }

	/// <summary>The list of actions with their bindings.</summary>
	public ControlsList List { get; }

	/// <summary>The view showing: the Layout or the List.</summary>
	public ControlsView View
	{
		get { return listView.Visible ? ControlsView.List : ControlsView.Layout; }
		set
		{
			var list = value == ControlsView.List;
			listView.Visible = list;
			layout.Visible = !list;
			keyboard.Visible = gamepad.Visible = categories.Visible = !list;
			Pick(layoutTab, !list);
			Pick(listTab, list);
			if (list)
			{
				List.Refresh();
			}
		}
	}

	/// <summary>Which device it shows. It starts on the one the player used last.</summary>
	public ControlScheme Scheme
	{
		get { return Keymap.Scheme; }
		set
		{
			Keymap.Scheme = value;
			Pick(keyboard, value == ControlScheme.KeyboardMouse);
			Pick(gamepad, value == ControlScheme.Gamepad);
			ShowControl(null);
		}
	}

	/// <summary>
	/// Opens the screen over the game, titled <paramref name="title"/>, with the Layout, the List, or both as tabs.
	/// Null when there is no UI to show it in.
	/// </summary>
	public static KeyBindingScreen? Open(string title = "Controls", ControlsView views = ControlsView.Both)
	{
		var document = UiDocument.Create();
		return document == null ? null : new KeyBindingScreen(document, title, views);
	}

	/// <summary>Takes it off the screen, giving up a rebinding it waits for.</summary>
	public void Close()
	{
		if (!Document.IsOpen)
		{
			return;
		}

		Input.CancelRebind();
		Document.Close();
		Closed?.Invoke();
	}

	/// <summary>Lists a control's actions under the device; null lists none.</summary>
	public void ShowControl(InputControl? control)
	{
		shown = control;
		Details.Clear();
		if (control is not InputControl showing)
		{
			return;
		}

		Details.Add(new Label(showing.Label)).AddClass("kbs-control");
		var uses = Input.ControlUses(showing);
		if (uses.Count == 0)
		{
			Details.Add(new Label("Not used")).AddClass("kbs-unused");
		}
		foreach (var use in uses)
		{
			AddUse(use);
		}
	}

	private void AddUse(ControlUse use)
	{
		var row = Details.Add(new Panel("kbs-use"));
		row.Add(new Label(use.Label)).AddClass("kbs-action");
		var rebind = row.Add(new Button("Rebind"));
		rebind.AddClass("kbs-rebind");
		rebind.Clicked += () => Rebind(use);
	}

	private async void Rebind(ControlUse use)
	{
		status.Text = $"Press a new control for {use.Label}, or Escape to keep it";
		var control = await Input.Rebind(use.Action, use.Binding, use.Part, Scheme);
		status.Text = string.Empty;
		if (control == null || !Document.IsOpen)
		{
			return;
		}

		Rebound?.Invoke(use.Action, control.Value);
		ShowControl(control);
	}

	private void ResetAll()
	{
		Input.ResetBindings();
		List.Refresh();
		ShowControl(shown);
	}

	private void SetCategories(bool on)
	{
		Keymap.Colors = on ? KeymapColors.Categories : KeymapColors.Assigned;
		ShowLegend();
	}

	/// <summary>A swatch and a name for each colour the keys can have.</summary>
	private void ShowLegend()
	{
		Legend.Clear();
		if (Keymap.Colors == KeymapColors.Categories)
		{
			AddLegend("No category", "assigned", null);
			foreach (var category in Input.Categories)
			{
				AddLegend(category.Name, "assigned", category.Color);
			}
		}
		else
		{
			AddLegend("Assigned", "assigned", null);
		}
		AddLegend("Conflict", "conflict", null);
		AddLegend("Hidden", "hidden", null);
		AddLegend("Unused", "unused", null);
	}

	private void AddLegend(string name, string state, Color? color)
	{
		var entry = Legend.Add(new Panel("kbs-legend-entry"));
		var swatch = entry.Add(new Panel("keymap-key", "kbs-swatch", state));
		if (color is Color fill)
		{
			swatch.SetStyle($"background-color: {Css(fill)}");
		}
		entry.Add(new Label(name));
	}

	private static void Pick(Button tab, bool picked)
	{
		if (picked)
		{
			tab.AddClass("selected");
		}
		else
		{
			tab.RemoveClass("selected");
		}
	}

	private static string Css(Color color)
	{
		static int Byte(float channel) => (int)MathF.Round(Math.Clamp(channel, 0f, 1f) * 255f);
		return $"#{Byte(color.R):x2}{Byte(color.G):x2}{Byte(color.B):x2}";
	}
}
