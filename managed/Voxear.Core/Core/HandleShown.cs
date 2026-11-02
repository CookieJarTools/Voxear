using System.Numerics;

namespace Voxear
{
	/// <summary>
	/// A handle shown this frame, for the editor to draw and take hold of: the behaviour that showed it, by its Obj's
	/// entity and its type, which of its handles it is, what kind, and where. Number is a radius or a distance along
	/// Direction.
	/// </summary>
	internal readonly record struct HandleShown(
		ulong Entity,
		string Type,
		int Id,
		int Kind,
		Vector3 At,
		Vector3 Direction,
		float Number,
		Color? Color);
}
