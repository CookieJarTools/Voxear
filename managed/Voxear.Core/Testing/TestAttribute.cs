using System;

namespace Voxear.Testing;

/// <summary>
/// Makes a static method that takes nothing and returns nothing a test, listed in the editor's Tests window to run
/// alone, with the rest of its category, or with every test: [Test("Doors open for keys", Category = "Doors")].
/// A test passes when it returns and fails when an <see cref="Assert"/> does not hold or it throws. Tests run in
/// edit mode against the level being edited, or in the running game while it plays, as commands do.
/// </summary>
[AttributeUsage(AttributeTargets.Method)]
public sealed class TestAttribute : Attribute
{
	public TestAttribute()
	{
	}

	public TestAttribute(string title)
	{
		Title = title;
	}

	/// <summary>What the Tests window calls it; the method's name when empty.</summary>
	public string Title { get; } = string.Empty;

	/// <summary>The group it is listed and run in, such as "Combat" or "Saving"; "General" when empty.</summary>
	public string Category { get; set; } = string.Empty;

	/// <summary>Where it runs among its category's tests, lowest first; tests of the same order go by title.</summary>
	public int Order { get; set; }
}
