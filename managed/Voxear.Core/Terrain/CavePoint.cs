using System.Numerics;

namespace Voxear;

/// <summary>
/// A place on a cave room's rock: where it is, and the way out of the rock there, one long. Read from a
/// <see cref="CaveRoom"/>'s wall, floor and ceiling points.
/// </summary>
public readonly record struct CavePoint(Vector3 Position, Vector3 Normal)
{
	/// <summary>How to turn something set here so its up is the way out of the rock.</summary>
	public Quaternion Rotation
	{
		get
		{
			var dot = Vector3.Dot(Vector3.UnitY, Normal);
			if (dot < -0.9999f)
			{
				return Quaternion.CreateFromAxisAngle(Vector3.UnitX, System.MathF.PI);
			}
			var axis = Vector3.Cross(Vector3.UnitY, Normal);
			return Quaternion.Normalize(new Quaternion(axis, 1f + dot));
		}
	}
}
