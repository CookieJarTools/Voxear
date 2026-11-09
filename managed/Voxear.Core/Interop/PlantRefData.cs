using System.Runtime.InteropServices;

namespace Voxear.Interop;

/// <summary>A plant, by its Obj and its number. Mirrors PlantRef in crates/voxear_scripting/src/plants.rs.</summary>
[StructLayout(LayoutKind.Sequential)]
internal struct PlantRefData
{
	public ulong Obj;
	public uint Number;
	public uint Pad;
}
