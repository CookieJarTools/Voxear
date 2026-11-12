using System.Runtime.InteropServices;

namespace Voxear.Interop;

/// <summary>
/// A particle's strike, as the engine tells of it: its system's Obj, which of its effect's emitters, how fast it
/// went in, where and the way the surface faces. Mirrors ParticleHitData in crates/voxear_scripting/src/physics.rs.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct ParticleHitData
{
	public ulong Obj;
	public uint Emitter;
	public float Speed;
	public float PointX;
	public float PointY;
	public float PointZ;
	public float NormalX;
	public float NormalY;
	public float NormalZ;
}
