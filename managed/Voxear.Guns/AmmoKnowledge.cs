namespace Voxear;

/// <summary>What a holder knows of how many rounds are in a gun's feed or chamber.</summary>
public enum AmmoKnowledge
{
	/// <summary>Not looked at since it was last fired from.</summary>
	Unknown,

	/// <summary>Judged by eye or weight: about empty, under half, about half, over half or about full.</summary>
	Rough,

	/// <summary>Counted, as a small magazine or a revolver's cylinder can be, or a gun just filled.</summary>
	Exact,
}
