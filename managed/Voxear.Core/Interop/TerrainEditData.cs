using System.Numerics;
using System.Runtime.InteropServices;

namespace Voxear.Interop;

/// <summary>An edit to the ground. Mirrors TerrainEditData in crates/voxear_scripting/src/terrain.rs.</summary>
[StructLayout(LayoutKind.Sequential)]
internal struct TerrainEditData
{
	public int Kind;
	public int Cut;
	public Vector3 At;
	public float Radius;
	public float Amount;
	public uint Pad;
}
