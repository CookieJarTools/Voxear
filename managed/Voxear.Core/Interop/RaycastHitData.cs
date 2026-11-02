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
	public uint Pad;
}
