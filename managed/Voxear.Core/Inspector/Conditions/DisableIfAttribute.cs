namespace Voxear
{
	/// <summary>Greys a field out, so it cannot be edited, while the condition holds. See <see cref="ConditionAttribute"/>.</summary>
	public sealed class DisableIfAttribute : ConditionAttribute
	{
		public DisableIfAttribute(string member, object? value = null)
			: base(member, value)
		{
		}
	}
}
