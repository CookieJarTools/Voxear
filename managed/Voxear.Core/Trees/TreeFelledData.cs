using System.Numerics;
using System.Runtime.InteropServices;

namespace Voxear.Interop;

/// <summary>A tree that came down, as the engine writes it: FelledData in crates/voxear_scripting/src/trees.rs.</summary>
[StructLayout(LayoutKind.Sequential)]
internal struct TreeFelledData
{
	public ulong World;
	public Vector3 Cut;
	public Vector3 Away;
	public Vector3 Landed;
	public int FirstDrop;
	public int DropCount;
}
