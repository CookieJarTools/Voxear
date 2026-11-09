using System.Runtime.InteropServices;

namespace Voxear.Interop;

/// <summary>Where a ray met a collider. Mirrors RaycastHitData in crates/voxear_scripting/src/physics.rs.</summary>
[StructLayout(LayoutKind.Sequential)]
internal struct RaycastHitData
{
	public ulong Obj;
	public float PointX;
	public float PointY;
	public float PointZ;
	public float NormalX;
	public float NormalY;
	public float NormalZ;
	public float Distance;
	/// <summary>The plant whose trunk it met, its number and one; 0 for none.</summary>
	public uint Plant;
}
