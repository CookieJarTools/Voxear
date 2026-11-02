namespace Voxear;

/// <summary>Shows the fields of the group named in the Inspector as a section that opens and closes, open to begin with.</summary>
public sealed class FoldoutGroupAttribute : GroupAttribute
{
	public FoldoutGroupAttribute(string name)
		: base(name)
	{
	}
}
