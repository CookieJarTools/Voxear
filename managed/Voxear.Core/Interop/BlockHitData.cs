using System.Runtime.InteropServices;

namespace Voxear.Interop;

/// <summary>Where a ray met a block. Mirrors BlockHitData in crates/voxear_scripting/src/blocks.rs.</summary>
[StructLayout(LayoutKind.Sequential)]
internal struct BlockHitData
{
	public ulong Obj;
	public float PointX;
	public float PointY;
	public float PointZ;
	public float NormalX;
	public float NormalY;
	public float NormalZ;
	public float MiddleX;
	public float MiddleY;
	public float MiddleZ;
	public float Distance;
	public float BlockSize;
	public uint Block;
}
