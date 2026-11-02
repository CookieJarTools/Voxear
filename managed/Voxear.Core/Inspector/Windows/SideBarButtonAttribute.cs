using System;

namespace Voxear;

/// <summary>
/// Puts a static method on the editor's side bar, the strip down its left edge, as a button with an icon that runs
/// it, as [Command] puts one in the command bar: <c>[SideBarButton(IconName.Grid, "Snap Everything")]</c>. The title
/// shows when the pointer rests on the button. Methods given the same Menu share one button, with the icon of the
/// first, which opens a menu of them. The method takes nothing and returns nothing, and runs as a command does:
/// against the level being edited, as one step to undo, or against the game while it plays.
/// <para>
/// For more than a button, such as something that shows what the game is doing, derive from SideBarItem.
/// </para>
/// </summary>
[AttributeUsage(AttributeTargets.Method, Inherited = false)]
public sealed class SideBarButtonAttribute : Attribute
{
	public SideBarButtonAttribute(IconName icon, string title)
	{
		Icon = icon;
		Title = title;
	}

	public IconName Icon { get; }

	/// <summary>What the button's tooltip says, or its line in a menu.</summary>
	public string Title { get; }

	/// <summary>The menu it is a line of, which every method with the same Menu shares; a button of its own when null.</summary>
	public string? Menu { get; init; }
}
