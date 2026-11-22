using System;

namespace Voxear;

/// <summary>
/// Makes a static method a call that tools make: <c>vox editor call</c> and <c>vox mcp</c> list it beside the
/// editor's own calls, with its arguments, and call it by its name, so an AI can drive the game's own systems as it
/// drives the engine's.
/// <code>
/// [Call("society.residents", About = "The residents near a place: each one's name, home and what they are doing.")]
/// public static List&lt;ResidentInfo&gt; Residents(Vector3 near, float reach = 100f)
/// {
///     return Society.Near(near, reach).Select(resident => resident.Info).ToList();
/// }
/// </code>
/// <para>
/// Its arguments are read by name from the call's: text, toggles, numbers, enums by name, vectors as [x, y, z], Objs
/// as a tool names them, and anything else as a save writes it. One with a default may be left out. What it returns
/// is given back as a save writes it: numbers, text, lists, dictionaries, classes by their fields.
/// </para>
/// </summary>
[AttributeUsage(AttributeTargets.Method)]
public sealed class CallAttribute : Attribute
{
	public CallAttribute(string name)
	{
		Name = name;
	}

	/// <summary>The call's name, as <c>society.residents</c>: lower case, its parts joined by dots.</summary>
	public string Name { get; }

	/// <summary>What it does and gives back, for a tool to read; the method's name when empty.</summary>
	public string About { get; set; } = string.Empty;

	/// <summary>When it can be called: while editing, while the game plays, or both.</summary>
	public CallWhen When { get; set; } = CallWhen.EditingAndPlaying;

	/// <summary>
	/// Whether it changes the level, so the editor lets it run only where its settings let tools change the
	/// project, and keeps what it changed while editing as one step to undo.
	/// </summary>
	public bool Changes { get; set; }
}
