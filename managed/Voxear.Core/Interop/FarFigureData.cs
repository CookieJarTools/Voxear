using System.Numerics;
using System.Runtime.InteropServices;

namespace Voxear.Interop;

/// <summary>
/// A figure drawn far off, as the engine reads it: its prefab's engine id, its number, where it is, which way it faces
/// and how fast it walks. Mirrors FarFigureData in crates/voxear_scripting/src/crowds.rs.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct FarFigureData
{
	public int Prefab;
	public uint Id;
	public Vector3 Position;
	public float Yaw;
	public float Speed;
}
