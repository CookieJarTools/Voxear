using System;

namespace Voxear
{
	/// <summary>
	/// What ListView and TreeView share: a scrolling list of rows, each made once and filled with an item whenever it
	/// shows another. Only the rows on screen exist, so a list of a hundred thousand items costs what one of twenty
	/// does. Every row has the list-row class, and matches :selected while selected, for sheets to style.
	/// </summary>
	public abstract class CollectionView : UiElement
	{
		private float itemHeight = 24f;

		private protected CollectionView(UiKind kind) : base(kind, string.Empty)
		{
		}

		/// <summary>How tall each row is, in pixels.</summary>
		public float ItemHeight
		{
			get { return itemHeight; }
			set
			{
				itemHeight = Math.Max(1f, value);
				Refresh();
			}
		}

		/// <summary>Runs each time the selection changes from the screen, at the start of the next frame.</summary>
		public event Action? SelectionChanged
		{
			add { Changed += value; }
			remove { Changed -= value; }
		}

		/// <summary>Fills every row on screen again. Call it after changing the items, as after adding one.</summary>
		public void Refresh()
		{
			if (IsShown)
			{
				SendItems();
			}
		}

		/// <summary>Makes what a row shows, once for each row element.</summary>
		internal abstract void Make(UiElement row);

		/// <summary>Fills a row with an item: a list's by its index, a tree's by its id.</summary>
		internal abstract void Bind(UiElement row, ulong item);

		/// <summary>Tells the engine what the items are now, which fills every row on screen again.</summary>
		private protected abstract void SendItems();

		/// <summary>What a row shows when nothing else makes it: a label with the item's text.</summary>
		private protected static void MakeLabel(UiElement row)
		{
			row.Add(new Label());
		}

		private protected static void ShowText(UiElement row, string? text)
		{
			if (row.Children.Count > 0)
			{
				row.Children[0].Text = text ?? string.Empty;
			}
		}

		private protected override void OnShown()
		{
			UiLists.Shown(this);
			SendItems();
		}

		private protected override void OnTakenOff()
		{
			UiLists.TakenOff(this);
		}
	}
}
