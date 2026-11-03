using System;

namespace Voxear;

/// <summary>
/// What an EditorWindow is called in the editor's menus and on its tab, and the icon beside it, one of Voxear UI's:
/// <c>[Window("Level Tools", Icon = IconName.Settings)]</c>, as [SideBarButton] takes one. Without it, a window is
/// called by its class name in words, with the editor's icon for a window.
/// </summary>
[AttributeUsage(AttributeTargets.Class, Inherited = false)]
public sealed class WindowAttribute : Attribute
{
	private readonly IconName? icon;

	public WindowAttribute(string title)
	{
		Title = title;
	}

	public string Title { get; }

	/// <summary>The icon beside its title; the editor's icon for a window when none is given.</summary>
	public IconName Icon
	{
		get { return icon ?? default; }
		init { icon = value; }
	}

	/// <summary>Whether an icon was given.</summary>
	internal bool HasIcon => icon != null;
}
