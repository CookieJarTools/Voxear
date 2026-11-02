namespace Voxear;

/// <summary>
/// Shows a field on one tab of a group of tabs in the Inspector: [TabGroup("Movement")] puts it on the Movement
/// tab of the behaviour's tabs, and [TabGroup("Settings", "Movement")] on that tab of the tabs called Settings.
/// </summary>
public sealed class TabGroupAttribute : GroupAttribute
{
	public TabGroupAttribute(string tab)
		: this("Tabs", tab)
	{
	}

	public TabGroupAttribute(string group, string tab)
		: base(group)
	{
		Tab = tab;
	}

	public string Tab { get; }
}
