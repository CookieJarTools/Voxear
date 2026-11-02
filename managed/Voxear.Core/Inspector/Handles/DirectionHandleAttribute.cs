namespace Voxear
{
	/// <summary>
	/// An arrow from the Obj in the Scene view for a Vector3 field, its tip dragged to point it elsewhere and change
	/// its length: <c>[DirectionHandle] public Vector3 Launch = new(0, 5, 5);</c>. It turns with the Obj unless Space is
	/// World.
	/// </summary>
	public sealed class DirectionHandleAttribute : HandleAttribute
	{
		public HandleSpace Space { get; init; }
	}
}
