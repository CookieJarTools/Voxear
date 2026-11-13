using System.Runtime.InteropServices;

namespace Voxear.Interop;

/// <summary>
/// A Heat Body's change or a character's feeling, as the engine tells of it: the Obj, what it became, which (0 a
/// change, 1 a feeling) and its number. Mirrors HeatEventData in crates/voxear_scripting/src/heat.rs.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct HeatEventData
{
	public ulong Obj;
	public ulong Became;
	public int Kind;
	public int Value;
}
