using System;
using System.Collections.Generic;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A scrolling list of items, as Unity's ListView: <c>hud.Root.Add(new ListView&lt;string&gt;(names))</c> shows each
/// name in a row. Give it MakeRow and BindRow to show more than text, and call Refresh after changing the items.
/// <code>
/// var players = hud.Root.Add(new ListView&lt;Player&gt;(team)
/// {
///     MakeRow = row => row.Add(new Label()),
///     BindRow = (row, player) => row.Children[0].Text = $"{player.Name}  {player.Score}",
/// });
/// players.SelectionChanged += () => Log.Info($"{players.SelectedItem?.Name} picked");
/// </code>
/// </summary>
public unsafe class ListView<T> : CollectionView
{
	private IList<T> items;
	private int selected = -1;

	public ListView(IList<T> items) : base(UiKind.ListView)
	{
		this.items = items;
	}

	/// <summary>The items, in order. Setting them fills the rows again; after changing the list itself, call Refresh.</summary>
	public IList<T> Items
	{
		get { return items; }
		set
		{
			items = value;
			Refresh();
		}
	}

	/// <summary>Adds what a row shows to it, once for each row: <c>row => row.Add(new Label())</c>. Without it, a row is a label.</summary>
	public Action<UiElement>? MakeRow { get; set; }

	/// <summary>Shows an item in a row MakeRow made. Without it, the row's first element shows the item's text.</summary>
	public Action<UiElement, T>? BindRow { get; set; }

	/// <summary>The selected item's index, or -1 for none.</summary>
	public int SelectedIndex
	{
		get { return IsShown ? (int)Value : selected; }
		set
		{
			selected = value;
			if (IsShown)
			{
				Value = value;
			}
		}
	}

	/// <summary>The selected item, or the default when nothing is selected.</summary>
	public T? SelectedItem
	{
		get
		{
			var index = SelectedIndex;
			return index >= 0 && index < items.Count ? items[index] : default;
		}
	}

	internal override void Make(UiElement row)
	{
		if (MakeRow != null)
		{
			MakeRow(row);
			return;
		}
		MakeLabel(row);
	}

	internal override void Bind(UiElement row, ulong item)
	{
		if (item >= (ulong)items.Count)
		{
			return;
		}
		if (BindRow != null)
		{
			BindRow(row, items[(int)item]);
			return;
		}
		ShowText(row, items[(int)item]?.ToString());
	}

	private protected override void SendItems()
	{
		Native.Api->UiItems(Id, items.Count, null, ItemHeight);
		if (selected >= 0)
		{
			Value = selected;
		}
	}
}
