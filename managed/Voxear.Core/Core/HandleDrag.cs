using System.Numerics;

namespace Voxear
{
	/// <summary>
	/// The handle the editor's pointer is dragging, by the behaviour that showed it and which of its handles it is, and
	/// where to: a point in the world, or a radius or distance.
	/// </summary>
	internal readonly record struct HandleDrag(ulong Entity, string Type, int Id, Vector3 Point, float Number);
}
