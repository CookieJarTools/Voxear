using System;
using System.Collections.Generic;
using System.Linq;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A scrolling tree of items, each opened and closed with its arrow, as Unity's TreeView: the items at the top,
/// and how to find what is in each. Give it MakeRow and BindRow to show more than text, and call Refresh after
/// changing the items.
/// <code>
/// var folders = hud.Root.Add(new TreeView&lt;Folder&gt;(drive.Folders, folder => folder.Folders));
/// folders.SelectionChanged += () => Open(folders.SelectedItem);
/// </code>
/// <para>
/// Each item is known by itself, as a dictionary key is, so it stays open and selected across a Refresh. An item
/// that shows twice, or inside itself, shows only the first time.
/// </para>
/// </summary>
public unsafe class TreeView<T> : CollectionView
	where T : notnull
{
	private readonly Dictionary<ulong, T> items = new Dictionary<ulong, T>();
	private Dictionary<T, ulong> ids = new Dictionary<T, ulong>();
	private IEnumerable<T> roots;
	private Func<T, IEnumerable<T>> childrenOf;
	private ulong nextId = 1;
	private T? selected;

	public TreeView(IEnumerable<T> roots, Func<T, IEnumerable<T>> childrenOf) : base(UiKind.TreeView)
	{
		(this.roots, this.childrenOf) = (roots, childrenOf);
	}

	/// <summary>The items at the top, in order. Setting them fills the rows again.</summary>
	public IEnumerable<T> Roots
	{
		get { return roots; }
		set
		{
			roots = value;
			Refresh();
		}
	}

	/// <summary>What is in an item, in order; nothing for one with no arrow.</summary>
	public Func<T, IEnumerable<T>> ChildrenOf
	{
		get { return childrenOf; }
		set
		{
			childrenOf = value;
			Refresh();
		}
	}

	/// <summary>Adds what a row shows to it, once for each row. Without it, a row is a label.</summary>
	public Action<UiElement>? MakeRow { get; set; }

	/// <summary>Shows an item in a row MakeRow made. Without it, the row's first element shows the item's text.</summary>
	public Action<UiElement, T>? BindRow { get; set; }

	/// <summary>The selected item, or the default when nothing is selected. Setting it opens what it is in.</summary>
	public T? SelectedItem
	{
		get
		{
			if (!IsShown)
			{
				return selected;
			}
			var id = Value;
			return id > 0 && items.TryGetValue((ulong)id, out var item) ? item : default;
		}
		set
		{
			selected = value;
			if (IsShown)
			{
				Value = value != null && ids.TryGetValue(value, out var id) ? id : -1;
			}
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
		if (!items.TryGetValue(item, out var value))
		{
			return;
		}
		if (BindRow != null)
		{
			BindRow(row, value);
			return;
		}
		ShowText(row, value.ToString());
	}

	/// <summary>Every item, depth first, as its id and its parent's, 0 for one at the top.</summary>
	private protected override void SendItems()
	{
		var pairs = Collect();
		fixed (ulong* written = pairs)
		{
			Native.Api->UiItems(Id, pairs.Length / 2, written, ItemHeight);
		}
		if (selected != null && ids.TryGetValue(selected, out var id))
		{
			Value = id;
		}
	}

	private ulong[] Collect()
	{
		items.Clear();
		var known = ids;
		ids = new Dictionary<T, ulong>();
		var pairs = new List<ulong>();
		var waiting = new Stack<(T Item, ulong Parent)>();
		Push(waiting, roots, 0);
		while (waiting.Count > 0)
		{
			var (item, parent) = waiting.Pop();
			if (ids.ContainsKey(item))
			{
				continue;
			}
			var id = known.TryGetValue(item, out var kept) ? kept : nextId++;
			(ids[item], items[id]) = (id, item);
			pairs.Add(id);
			pairs.Add(parent);
			Push(waiting, childrenOf(item), id);
		}
		return pairs.ToArray();
	}

	/// <summary>Pushes a level's items so they come off in order.</summary>
	private static void Push(Stack<(T Item, ulong Parent)> waiting, IEnumerable<T> level, ulong parent)
	{
		foreach (var item in level.Reverse())
		{
			waiting.Push((item, parent));
		}
	}
}
