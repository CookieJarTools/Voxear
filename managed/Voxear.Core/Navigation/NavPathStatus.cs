namespace Voxear;

/// <summary>Whether a way was found, and how much of one.</summary>
public enum NavPathStatus
{
	/// <summary>No way: there is no place to start from, or nothing has been asked.</summary>
	None,

	/// <summary>All the way to the place nearest the destination.</summary>
	Complete,

	/// <summary>As near the destination as there is a way to: it cannot be reached, or not through the ground made so far.</summary>
	Partial,

	/// <summary>The first part of a long way that goes on to the destination. An agent finds the rest as it nears the end.</summary>
	Continues,
}
