using System;
using System.Collections.Generic;
using System.Linq;

namespace Voxear;

/// <summary>
/// A ready-made menu of texture packs for players: a row for each pack there is to wear, with what it looks like,
/// the one worn marked, and None at the top. Clicking a row wears it at once, so the player sees the game change
/// behind the menu, and the choice is kept for the game's next run. Open it from a pause or settings menu with
/// <c>TexturePackScreen.Open()</c>; it closes with its Close button.
/// <code>
/// var screen = TexturePackScreen.Open("Textures");
/// screen.Picked += pack => Log.Info($"now wearing {pack}");
/// </code>
/// <para>
/// New Pack makes a pack of the player's own to start from (TexturePacks.WriteStarter) and lists it. The screen is
/// built from the same pieces a screen of your own would be, so it can be changed rather than replaced: the Bar,
/// the List and the Document they are in, whose sheets style its classes, <c>texture-pack-screen</c> and those
/// starting <c>tps-</c>.
/// </para>
/// </summary>
public sealed class TexturePackScreen
{
	private const string StarterName = "My Pack";

	private readonly List<(TexturePack Pack, Button Row)> rows = [];
	private readonly Label folder;

	private TexturePackScreen(UiDocument document, string title)
	{
		Document = document;
		var screen = document.Root.Add(new Panel("texture-pack-screen"));
		Bar = screen.Add(new Panel("tps-bar"));
		Bar.Add(new Label(title)).AddClass("tps-title");
		Bar.Add(new Panel("tps-spacer"));
		var starter = Bar.Add(new Button("New Pack"));
		var refresh = Bar.Add(new Button("Refresh"));
		var close = Bar.Add(new Button("Close"));
		var scroll = screen.Add(new ScrollView());
		scroll.AddClass("tps-scroll");
		List = scroll.Add(new Panel("tps-list"));
		folder = screen.Add(new Label(string.Empty));
		folder.AddClass("tps-folder");
		starter.Clicked += WriteStarter;
		refresh.Clicked += Refresh;
		close.Clicked += Close;
		Refresh();
	}

	/// <summary>The player picked a pack, which is worn now; TexturePack.None for none.</summary>
	public event Action<TexturePack>? Picked;

	/// <summary>The screen was closed.</summary>
	public event Action? Closed;

	/// <summary>The UI document the screen is in.</summary>
	public UiDocument Document { get; }

	/// <summary>The bar along the top: the title, New Pack, Refresh and Close.</summary>
	public Panel Bar { get; }

	/// <summary>The rows, one a pack, under None.</summary>
	public Panel List { get; }

	/// <summary>
	/// Opens the screen over the game, titled <paramref name="title"/>. Null when there is no UI to show it in.
	/// </summary>
	public static TexturePackScreen? Open(string title = "Textures")
	{
		var document = UiDocument.Create();
		return document == null ? null : new TexturePackScreen(document, title);
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

	/// <summary>Lists the packs again, as after the player put a folder in the Textures folder.</summary>
	public void Refresh()
	{
		List.Clear();
		rows.Clear();
		var worn = TexturePacks.Current;
		AddRow(TexturePack.None, "None", "Every block and material with its own pictures.", worn);
		foreach (var pack in TexturePacks.All)
		{
			AddRow(pack, pack.Name, pack.Description, worn);
		}

		var where = TexturePacks.Folder;
		folder.Text = where == null ? string.Empty : $"Packs of your own go in {where}, a folder each";
	}

	private void WriteStarter()
	{
		var taken = TexturePacks.All.Where(pack => pack.Source == TexturePackSource.Player).Select(pack => pack.Name);
		var names = new HashSet<string>(taken, StringComparer.OrdinalIgnoreCase);
		var name = StarterName;
		for (var number = 2; names.Contains(name); number++)
		{
			name = $"{StarterName} {number}";
		}

		if (TexturePacks.WriteStarter(name).IsSet)
		{
			Refresh();
		}
	}

	private void AddRow(TexturePack pack, string name, string description, TexturePack worn)
	{
		var row = List.Add(new Button(string.Empty));
		row.AddClass("tps-row");
		row.Add(new Label(name)).AddClass("tps-name");
		if (pack.Source == TexturePackSource.Player)
		{
			row.Add(new Label("Yours")).AddClass("tps-source");
		}
		row.Add(new Label(description)).AddClass("tps-description");
		Mark(row, pack == worn);
		row.Clicked += () => Pick(pack);
		rows.Add((pack, row));
	}

	private void Pick(TexturePack pack)
	{
		TexturePacks.Use(pack);
		foreach (var (listed, row) in rows)
		{
			Mark(row, listed == pack);
		}

		Picked?.Invoke(pack);
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
