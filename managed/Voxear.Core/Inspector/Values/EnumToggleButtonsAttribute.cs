using System;

namespace Voxear
{
	/// <summary>
	/// Shows an enum field as a row of buttons, one per value, instead of a dropdown.
	/// </summary>
	[AttributeUsage(AttributeTargets.Field)]
	public sealed class EnumToggleButtonsAttribute : Attribute
	{
	}
}
