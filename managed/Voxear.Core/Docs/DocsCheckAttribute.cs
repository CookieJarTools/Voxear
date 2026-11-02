using System;

namespace Voxear;

/// <summary>
/// Makes a static method a check a VoxDocs guide can show: a step that looks at the level or the project and ticks
/// itself while it passes, as "The scene is lit" or "Every door has a key".
/// <para>
/// The method takes nothing and returns <c>bool</c>, true when it passes, or <c>string</c>: nothing when it passes,
/// or else what is still to do, in words for the reader, which the page shows under the step. While editing it runs
/// against the level being edited, and while playing against the game; it should only look, not change anything.
/// </para>
/// <para>
/// A page shows it with a link on its own line, naming its class and method:
/// <c>[The lamps are lit](check:code/Hello.Tools.LampsAreLit)</c>. While writing a page, Action… lists every check
/// marked with this, to put one in at the cursor.
/// </para>
/// </summary>
[AttributeUsage(AttributeTargets.Method)]
public sealed class DocsCheckAttribute : Attribute
{
	public DocsCheckAttribute(string title)
	{
		Title = title;
	}

	/// <summary>What the step says, unless the page says otherwise.</summary>
	public string Title { get; }
}
