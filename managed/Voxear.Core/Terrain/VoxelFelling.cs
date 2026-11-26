namespace Voxear;

/// <summary>
/// How an endless Voxel Volume's felled trees fall and come to rest, and whether clippings fly up from its cut grass
/// (<see cref="Voxels.Felling"/>). Saved with the scene.
/// <code>
/// // Heavier, slower-tipping trees that settle quickly, and the game's own effect for cut grass.
/// Voxels.Felling = Voxels.Felling with { Push = 0.3f, WoodDensity = 900f, RestAfter = 0.5f, Clippings = false };
/// </code>
/// </summary>
/// <param name="Push">How fast a felled tree is set turning over the edge of its foot the way it was cut, in radians a
/// second: a slight push, gravity doing the rest.</param>
/// <param name="RestAfter">How long a felled piece must lie still to be at rest, in seconds.</param>
/// <param name="PressDepth">How far over the ground a resting crown's leaves are pressed down, in metres.</param>
/// <param name="PressTo">What share of that height they are pressed to, from 0 (flat) to 1 (not at all).</param>
/// <param name="WoodDensity">How heavy the wood is, in kilograms a cubic metre.</param>
/// <param name="Clippings">Whether clippings fly up where grass is cut; off, <see cref="VoxelCutDone.Mown"/> says
/// where, for a game's own effect.</param>
public readonly record struct VoxelFelling(
	float Push = 0.6f,
	float RestAfter = 1f,
	float PressDepth = 0.7f,
	float PressTo = 0.45f,
	float WoodDensity = 650f,
	bool Clippings = true)
{
	/// <summary>As a volume comes.</summary>
	public VoxelFelling() : this(0.6f)
	{
	}
}
