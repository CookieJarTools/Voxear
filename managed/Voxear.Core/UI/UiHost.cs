using System;
using System.Collections.Generic;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// Where the elements whose text or value comes from code (Bind, Localize, a LocalizedString) are kept up to date:
/// the game's UI, at the start of each frame, and each drawer and editor window as it draws, since their elements
/// are the editor's and can only be reached then. An element joins the host that is drawing when it shows bound,
/// or the game's when none is, and leaves it as it is taken off the screen.
/// </summary>
internal sealed class UiHost
{
	private static UiHost? drawing;

	private readonly HashSet<UiElement> bound = new HashSet<UiElement>();

	/// <summary>The game's own UI.</summary>
	public static UiHost Game { get; } = new UiHost();

	/// <summary>The host elements bound now join: the drawer or window drawing, or the game's.</summary>
	public static UiHost Current => drawing ?? Game;

	/// <summary>Makes it the host elements join until Leave, as a drawer draws; gives back the one before.</summary>
	public UiHost? Enter()
	{
		var before = drawing;
		drawing = this;
		return before;
	}

	public static void Leave(UiHost? before)
	{
		drawing = before;
	}

	public void Track(UiElement element)
	{
		bound.Add(element);
	}

	public void Untrack(UiElement element)
	{
		bound.Remove(element);
	}

	/// <summary>Works out each bound element's text and value again, each guarded on its own.</summary>
	public void Refresh()
	{
		if (bound.Count == 0)
		{
			return;
		}

		var version = NativeText.Version();
		foreach (var element in new List<UiElement>(bound))
		{
			try
			{
				element.RefreshBound(version);
			}
			catch (Exception exception)
			{
				Log.Exception(exception);
			}
		}
	}

	/// <summary>Lets go of every element, as when the world changes or the scripts reload.</summary>
	public void Forget()
	{
		bound.Clear();
	}
}
