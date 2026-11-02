using System.Runtime.InteropServices;

namespace Voxear.Interop;

/// <summary>
/// A contact or trigger crossing, as the Obj A is told of it, B being the other Obj. Mirrors PhysicsEventData in
/// crates/voxear_scripting/src/physics.rs. The normal points from B towards A.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct PhysicsEventData
{
	public const uint CollisionEnter = 0;
	public const uint CollisionExit = 1;
	public const uint TriggerEnter = 2;
	public const uint TriggerExit = 3;
	public const uint CollisionEnter2D = 4;
	public const uint CollisionExit2D = 5;
	public const uint TriggerEnter2D = 6;
	public const uint TriggerExit2D = 7;

	public ulong A;
	public ulong B;
	public uint Kind;
	public float PointX;
	public float PointY;
	public float PointZ;
	public float NormalX;
	public float NormalY;
	public float NormalZ;
	public uint Pad;
}
