using System;

namespace Voxear
{
	/// <summary>
	/// Offers the field's values from a menu, as a member of the behaviour named with nameof gives them: a method taking
	/// nothing, a property or a field, holding any list of values, or of ValueDropdownItems to give each one a name.
	/// </summary>
	[AttributeUsage(AttributeTargets.Field)]
	public sealed class ValueDropdownAttribute : Attribute
	{
		public ValueDropdownAttribute(string member)
		{
			Member = member;
		}

		public string Member { get; }
	}
}
