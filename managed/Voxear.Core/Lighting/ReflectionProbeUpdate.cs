namespace Voxear;

/// <summary>When a Reflection Probe is captured.</summary>
public enum ReflectionProbeUpdate
{
	/// <summary>Once, as the level opens, or from what was baked beside it; again only when Capture asks.</summary>
	Once,

	/// <summary>Again and again as the game goes, every Interval seconds, so it shows what moves.</summary>
	Live,
}
