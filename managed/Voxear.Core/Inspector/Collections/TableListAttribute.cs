using System;

namespace Voxear
{
	/// <summary>
	/// Shows a list of [Serializable] values as a table: a column for each of their fields, and a row for each item,
	/// rather than each item as a group of rows.
	/// </summary>
	[AttributeUsage(AttributeTargets.Field)]
	public sealed class TableListAttribute : Attribute
	{
	}
}
