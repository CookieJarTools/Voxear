using System;

namespace Voxear;

/// <summary>
/// What an EditorWindow is called in the editor's menus and on its tab, and the icon beside it, by Voxear UI's
/// name for one: <c>[Window("Level Tools", Icon = "wrench")]</c>. Without it, a window is called by its class name
/// in words.
/// </summary>
[AttributeUsage(AttributeTargets.Class, Inherited = false)]
public sealed class WindowAttribute : Attribute
{
	public WindowAttribute(string title)
	{
		Title = title;
	}

	public string Title { get; }

	public string? Icon { get; init; }
}
