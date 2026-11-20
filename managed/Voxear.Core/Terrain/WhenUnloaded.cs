namespace Voxear;

/// <summary>What happens to an Obj with Streams With Ground when the ground under it streams out.</summary>
public enum WhenUnloaded
{
	/// <summary>Taken away and kept, and made again as it was when its ground comes back.</summary>
	KeepForLater,

	/// <summary>Destroyed, and not made again.</summary>
	Destroy,

	/// <summary>Left as it is.</summary>
	Stay,
}
