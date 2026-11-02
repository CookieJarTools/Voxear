namespace Voxear;

/// <summary>Shows the fields of the group named in the Inspector as a box with the group's name as its title.</summary>
public sealed class BoxGroupAttribute : GroupAttribute
{
	public BoxGroupAttribute(string name)
		: base(name)
	{
	}
}
