using System;

namespace Voxear;

/// <summary>
/// Puts a static method in the editor's command palette (Ctrl+K), as Unity's MenuItem puts one in a menu:
/// [Command("Line Up Lamps", Keys = "Ctrl+Shift+L")]. A method the palette lists takes nothing and returns
/// nothing. It runs in edit mode against the level being edited, so it can find Objs and change their names and
/// transforms, and what it changes is one step to undo. Behaviours do not run in edit mode, so it finds none on
/// the Objs.
/// <para>
/// In a development build's console every command can be typed by its method's name, with what it takes after it,
/// as <c>SpawnWave 3 Wolves</c>: text, toggles, numbers, enums by name, and vectors as "x,y,z". What it returns is
/// printed.
/// </para>
/// </summary>
[AttributeUsage(AttributeTargets.Method)]
public sealed class CommandAttribute : Attribute
{
	public CommandAttribute(string title)
	{
		Title = title;
	}

	/// <summary>What the palette calls it.</summary>
	public string Title { get; }

	/// <summary>Keys that run it from anywhere in the editor, such as "Ctrl+Shift+L"; none when empty.</summary>
	public string Keys { get; set; } = string.Empty;
}
