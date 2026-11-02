using System.Runtime.InteropServices;

namespace Voxear.Interop;

/// <summary>
/// What a query asks the engine for: the components it hands out, in order, and its filters. The layout mirrors
/// QueryDesc in crates/voxear_scripting/src/ecs.rs.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal unsafe struct QueryDesc
{
	public uint* Fetch;
	public int FetchCount;
	public uint* With;
	public int WithCount;
	public uint* Without;
	public int WithoutCount;
	public uint* Changed;
	public int ChangedCount;
	public uint Since;
}
