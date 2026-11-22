using System.Runtime.InteropServices;

namespace Voxear.Interop;

/// <summary>
/// A column of a world of blocks without end made or let go, as the engine tells of it: the world's Obj, the column's
/// place, 1 made or 0 let go, its width in metres and its low corner along x and z. Mirrors ColumnEventData in
/// crates/voxear_scripting/src/columns.rs.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct ColumnEventData
{
	public ulong World;
	public int X;
	public int Z;
	public int Made;
	public float Size;
	public float CornerX;
	public float CornerZ;
}
