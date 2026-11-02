namespace Voxear;

/// <summary>
/// An arc in the Scene view for a float field of degrees, opening around the Obj's forward on both sides, as a
/// field of view, its end dragged to widen or narrow it: <c>[ArcHandle(Radius = 5)] public float ViewAngle = 90f;</c>.
/// </summary>
public sealed class ArcHandleAttribute : HandleAttribute
{
	/// <summary>How far from the Obj the arc is drawn, in metres.</summary>
	public float Radius { get; init; } = 2f;
}
