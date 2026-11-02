namespace Voxear
{
	/// <summary>
	/// A point along one of the Obj's own axes in the Scene view for a float field, dragged along it to change the
	/// distance: <c>[SliderHandle(Axis = HandleAxis.Y)] public float JumpHeight = 2f;</c>. Up, Y, unless Axis says.
	/// </summary>
	public sealed class SliderHandleAttribute : HandleAttribute
	{
		public HandleAxis Axis { get; init; } = HandleAxis.Y;
	}
}
