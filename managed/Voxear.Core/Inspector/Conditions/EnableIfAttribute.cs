namespace Voxear;

/// <summary>Lets a field be edited only while the condition holds; otherwise it shows greyed out. See <see cref="ConditionAttribute"/>.</summary>
public sealed class EnableIfAttribute : ConditionAttribute
{
	public EnableIfAttribute(string member, object? value = null)
		: base(member, value)
	{
	}
}
