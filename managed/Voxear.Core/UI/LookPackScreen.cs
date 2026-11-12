using System;
using System.Collections.Generic;

namespace Voxear;

/// <summary>
/// A ready-made menu of look packs for players: a row for each pack there is to wear, with what it looks like, the
/// one worn marked, and None at the top. Clicking a row wears it at once, so the player sees the game change behind
/// the menu, and the choice is kept for the game's next run. Open it from a pause or settings menu with
/// <c>LookPackScreen.Open()</c>; it closes with its Close button.
/// <code>
/// var screen = LookPackScreen.Open("Looks");
/// screen.Picked += pack => Log.Info($"now wearing {pack}");
/// </code>
/// <para>
/// It is built from the same pieces a screen of your own would be, so it can be changed rather than replaced: the
/// Bar, the List and the Document they are in, whose sheets style its classes, <c>look-pack-screen</c> and those
/// starting <c>lps-</c>.
/// </para>
/// </summary>
public sealed class LookPackScreen
{
	private readonly List<(LookPack Pack, Button Row)> rows = [];
	private readonly Label folder;

	private LookPackScreen(UiDocument document, string title)
	{
		Document = document;
		var screen = document.Root.Add(new Panel("look-pack-screen"));
		Bar = screen.Add(new Panel("lps-bar"));
		Bar.Add(new Label(title)).AddClass("lps-title");
		Bar.Add(new Panel("lps-spacer"));
		var refresh = Bar.Add(new Button("Refresh"));
		var close = Bar.Add(new Button("Close"));
		var scroll = screen.Add(new ScrollView());
		scroll.AddClass("lps-scroll");
		List = scroll.Add(new Panel("lps-list"));
		folder = screen.Add(new Label(string.Empty));
		folder.AddClass("lps-folder");
		refresh.Clicked += Refresh;
		close.Clicked += Close;
		Refresh();
	}

	/// <summary>The player picked a pack, which is worn now; LookPack.None for none.</summary>
	public event Action<LookPack>? Picked;

	/// <summary>The screen was closed.</summary>
	public event Action? Closed;

	/// <summary>The UI document the screen is in.</summary>
	public UiDocument Document { get; }

	/// <summary>The bar along the top: the title, Refresh and Close.</summary>
	public Panel Bar { get; }

	/// <summary>The rows, one a pack, under None.</summary>
	public Panel List { get; }

	/// <summary>
	/// Opens the screen over the game, titled <paramref name="title"/>. Null when there is no UI to show it in.
	/// </summary>
	public static LookPackScreen? Open(string title = "Looks")
	{
		var document = UiDocument.Create();
		return document == null ? null : new LookPackScreen(document, title);
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

	/// <summary>Lists the packs again, as after the player put a file in the Looks folder.</summary>
	public void Refresh()
	{
		List.Clear();
		rows.Clear();
		var worn = LookPacks.Current;
		AddRow(LookPack.None, "None", "Each level as it was made.", worn);
		foreach (var pack in LookPacks.All)
		{
			AddRow(pack, pack.Name, pack.Description, worn);
		}

		var where = LookPacks.Folder;
		folder.Text = where == null ? string.Empty : $"Packs of your own go in {where}";
	}

	private void AddRow(LookPack pack, string name, string description, LookPack worn)
	{
		var row = List.Add(new Button(string.Empty));
		row.AddClass("lps-row");
		row.Add(new Label(name)).AddClass("lps-name");
		if (pack.Source == LookPackSource.Player)
		{
			row.Add(new Label("Yours")).AddClass("lps-source");
		}
		row.Add(new Label(description)).AddClass("lps-description");
		Mark(row, pack == worn);
		row.Clicked += () => Pick(pack);
		rows.Add((pack, row));
	}

	private void Pick(LookPack pack)
	{
		LookPacks.Use(pack);
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
