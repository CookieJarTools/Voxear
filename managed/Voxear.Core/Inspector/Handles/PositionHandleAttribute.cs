namespace Voxear
{
	/// <summary>
	/// A point in the Scene view for a Vector3 field, dragged to move it: <c>[PositionHandle] public Vector3 Target;</c>.
	/// On a list of Vector3, every item gets a point, joined in order as a path. The point is the Obj's own, moving
	/// and turning with it, unless Space is World.
	/// </summary>
	public sealed class PositionHandleAttribute : HandleAttribute
	{
		public HandleSpace Space { get; init; }
	}
}
