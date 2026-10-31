namespace Voxear
{
	/// <summary>Hides a field in the Inspector while the condition holds. See <see cref="ConditionAttribute"/>.</summary>
	public sealed class HideIfAttribute : ConditionAttribute
	{
		public HideIfAttribute(string member, object? value = null)
			: base(member, value)
		{
		}
	}
}
