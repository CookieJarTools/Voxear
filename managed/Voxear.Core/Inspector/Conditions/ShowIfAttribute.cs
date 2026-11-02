namespace Voxear;

/// <summary>Shows a field in the Inspector only while the condition holds. See <see cref="ConditionAttribute"/>.</summary>
public sealed class ShowIfAttribute : ConditionAttribute
{
	public ShowIfAttribute(string member, object? value = null)
		: base(member, value)
	{
	}
}
