using System.Runtime.InteropServices;

namespace Voxear.Interop;

/// <summary>How a character's move went. Mirrors CharacterMoveData in crates/voxear_scripting/src/physics.rs.</summary>
[StructLayout(LayoutKind.Sequential)]
internal struct CharacterMoveData
{
	public const uint OnGround = 0;
	public const uint OnSteepGround = 1;
	public const uint Against = 2;
	public const uint InAir = 3;

	public float PositionX;
	public float PositionY;
	public float PositionZ;
	public float VelocityX;
	public float VelocityY;
	public float VelocityZ;
	public float GroundNormalX;
	public float GroundNormalY;
	public float GroundNormalZ;
	public uint Ground;
}
