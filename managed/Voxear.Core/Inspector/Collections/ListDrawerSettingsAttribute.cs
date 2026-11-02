using System;

namespace Voxear;

/// <summary>How the Inspector shows a list: a page of items at a time, and each item labelled by its index or not at all.</summary>
[AttributeUsage(AttributeTargets.Field)]
public sealed class ListDrawerSettingsAttribute : Attribute
{
	/// <summary>How many items show at a time, with buttons to turn the page; 0 shows them all.</summary>
	public int NumberOfItemsPerPage { get; set; }

	/// <summary>Labels each item with its index, as "0"; otherwise items have no label.</summary>
	public bool ShowIndexLabels { get; set; }
}
