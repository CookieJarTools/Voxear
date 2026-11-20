namespace Voxear;

/// <summary>When the world's middle moves with the viewer, as Project Settings ▸ Floating Origin has it.</summary>
public enum FloatingOriginMode
{
	/// <summary>Never: the middle stays where the level puts it.</summary>
	Off,

	/// <summary>In levels with ground without end only.</summary>
	Endless,

	/// <summary>In every level.</summary>
	Always,
}
