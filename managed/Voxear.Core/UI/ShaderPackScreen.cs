using System;
using System.Collections.Generic;
using System.Linq;

namespace Voxear;

/// <summary>
/// A ready-made menu of shader packs for players: a row for each pack there is to wear, with what it looks like, the
/// one worn marked, and None at the top; under them, the options of the pack worn, a slider, toggle or dropdown
/// each, and how its compile stands. Clicking a row wears it, so the player sees the game change behind the menu once
/// it has compiled, and the choice and the options set are kept for the game's next run. Open it from a pause or
/// settings menu with <c>ShaderPackScreen.Open()</c>; it closes with its Close button.
/// <code>
/// var screen = ShaderPackScreen.Open("Shaders");
/// screen.Picked += pack => Log.Info($"now wearing {pack}");
/// </code>
/// <para>
/// It is built from the same pieces a screen of your own would be, so it can be changed rather than replaced: the
/// Bar, the List, the Options and the Document they are in, whose sheets style its classes,
/// <c>shader-pack-screen</c> and those starting <c>sps-</c>.
/// </para>
/// </summary>
public sealed class ShaderPackScreen
{
	private readonly List<(ShaderPack Pack, Button Row)> rows = [];
	private readonly Label status;
	private readonly Label folder;
	private string shownOptions = string.Empty;

	private ShaderPackScreen(UiDocument document, string title)
	{
		Document = document;
		var screen = document.Root.Add(new Panel("shader-pack-screen"));
		Bar = screen.Add(new Panel("sps-bar"));
		Bar.Add(new Label(title)).AddClass("sps-title");
		Bar.Add(new Panel("sps-spacer"));
		var refresh = Bar.Add(new Button("Refresh"));
		var close = Bar.Add(new Button("Close"));
		var scroll = screen.Add(new ScrollView());
		scroll.AddClass("sps-scroll");
		List = scroll.Add(new Panel("sps-list"));
		status = screen.Add(new Label(string.Empty));
		status.AddClass("sps-status");
		Options = screen.Add(new Panel("sps-options"));
		folder = screen.Add(new Label(string.Empty));
		folder.AddClass("sps-folder");
		refresh.Clicked += Refresh;
		close.Clicked += Close;
		// Worked out each frame: how the compile stands, and the options again once the pack worn has changed.
		status.Bind(Standing);
		Refresh();
	}

	/// <summary>The player picked a pack, which is being put on now; ShaderPack.None for none.</summary>
	public event Action<ShaderPack>? Picked;

	/// <summary>The screen was closed.</summary>
	public event Action? Closed;

	/// <summary>The UI document the screen is in.</summary>
	public UiDocument Document { get; }

	/// <summary>The bar along the top: the title, Refresh and Close.</summary>
	public Panel Bar { get; }

	/// <summary>The rows, one a pack, under None.</summary>
	public Panel List { get; }

	/// <summary>The options of the pack worn, one row each.</summary>
	public Panel Options { get; }

	/// <summary>
	/// Opens the screen over the game, titled <paramref name="title"/>. Null when there is no UI to show it in.
	/// </summary>
	public static ShaderPackScreen? Open(string title = "Shaders")
	{
		var document = UiDocument.Create();
		return document == null ? null : new ShaderPackScreen(document, title);
	}

	/// <summary>Takes it off the screen.</summary>
	public void Close()
	{
		if (!Document.IsOpen)
		{
			return;
		}

		Document.Close();
		Closed?.Invoke();
	}

	/// <summary>Lists the packs again, as after the player put a folder in the ShaderPacks folder.</summary>
	public void Refresh()
	{
		List.Clear();
		rows.Clear();
		var worn = ShaderPacks.Current;
		AddRow(ShaderPack.None, "None", "Each level as the engine draws it.", worn);
		foreach (var pack in ShaderPacks.All)
		{
			AddRow(pack, pack.Name, pack.Description, worn);
		}

		var where = ShaderPacks.Folder;
		folder.Text = where == null ? string.Empty : $"Packs of your own go in {where}";
		shownOptions = string.Empty;
		Standing();
	}

	private void AddRow(ShaderPack pack, string name, string description, ShaderPack worn)
	{
		var row = List.Add(new Button(string.Empty));
		row.AddClass("sps-row");
		row.Add(new Label(name)).AddClass("sps-name");
		if (pack.Source == ShaderPackSource.Player)
		{
			row.Add(new Label("Yours")).AddClass("sps-source");
		}
		row.Add(new Label(description)).AddClass("sps-description");
		Mark(row, pack == worn);
		row.Clicked += () => Pick(pack);
		rows.Add((pack, row));
	}

	private void Pick(ShaderPack pack)
	{
		ShaderPacks.Use(pack);
		foreach (var (listed, row) in rows)
		{
			Mark(row, listed == pack);
		}

		Picked?.Invoke(pack);
	}

	/// <summary>How the pack worn stands, for the status line; and its options shown again when they have changed.</summary>
	private string Standing()
	{
		var options = ShaderPacks.Options;
		var names = ShaderPacks.Current.Reference + ":" + string.Join(",", options.Select(option => option.Name));
		if (names != shownOptions)
		{
			shownOptions = names;
			ShowOptions(options);
		}

		var standing = ShaderPacks.Standing;
		if (standing == ShaderPackStanding.Failed)
		{
			status.AddClass("failed");
			return $"It did not compile: {ShaderPacks.Error}";
		}

		status.RemoveClass("failed");
		return standing == ShaderPackStanding.Compiling ? "Compiling…" : string.Empty;
	}

	private void ShowOptions(IReadOnlyList<ShaderPackOption> options)
	{
		Options.Clear();
		foreach (var option in options)
		{
			var row = Options.Add(new Panel("sps-option"));
			var label = row.Add(new Label(option.Label));
			label.AddClass("sps-label");
			label.Tooltip = option.About;
			var name = option.Name;
			UiElement control = option.Kind switch
			{
				ShaderPackOptionKind.Toggle => new Toggle(string.Empty, option.IsOn).OnToggled(on => ShaderPacks.SetOption(name, on)),
				ShaderPackOptionKind.Choice => Chooser(option),
				_ => new Slider(option.Low, option.High, option.Value).OnMoved(value => ShaderPacks.SetOption(name, value)),
			};
			row.Add(control).AddClass("sps-control");
		}
	}

	private static Dropdown Chooser(ShaderPackOption option)
	{
		var name = option.Name;
		var chooser = new Dropdown([.. option.Choices]) { Selected = (int)MathF.Round(option.Value) };
		return chooser.OnPicked(at => ShaderPacks.SetOption(name, (float)at));
	}

	private static void Mark(Button row, bool worn)
	{
		if (worn)
		{
			row.AddClass("selected");
		}
		else
		{
			row.RemoveClass("selected");
		}
	}
}
