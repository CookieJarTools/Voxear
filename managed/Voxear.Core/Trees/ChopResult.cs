namespace Voxear;

/// <summary>What mining a log once did (see <see cref="Trees.Chop"/>).</summary>
public enum ChopResult
{
	/// <summary>Nothing: there was no log that chops there.</summary>
	None,
	/// <summary>It stands a slice thinner.</summary>
	Thinner,
	/// <summary>It was cut through and is gone; nothing came down.</summary>
	Cut,
	/// <summary>It was cut through, and the tree over it is coming down.</summary>
	Felled,
}
