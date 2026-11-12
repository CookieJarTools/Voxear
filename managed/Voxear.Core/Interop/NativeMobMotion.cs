using System.Numerics;

namespace Voxear.Interop;

/// <summary>
/// The engine's Mob Motions, for MobMotion: what each value's number is, as the engine's mob_motion.rs has them.
/// Without the engine, as in tests, no Obj has one, and every value reads as missing.
/// </summary>
internal static unsafe class NativeMobMotion
{
	public const int Has = 0;
	public const int Add = 1;
	public const int Remove = 2;
	public const int LookAt = 3;
	public const int Attack = 4;
	public const int Hurt = 5;
	public const int Velocity = 6;
	public const int LegSwing = 7;
	public const int ArmSwing = 8;
	public const int Stride = 9;
	public const int Bounce = 10;
	public const int Idle = 11;
	public const int LookSpeed = 12;
	public const int LookLimit = 13;
	public const int Lag = 14;

	/// <summary>A value of an Obj's Mob Motion, or false when the Obj has none.</summary>
	public static bool Get(Obj obj, int what, out Vector4 value)
	{
		var read = default(Vector4);
		var found = Native.IsConnected && Native.Api->MobMotionValue(obj.Id, what, 0, (float*)&read) == 1;
		value = read;
		return found;
	}

	/// <summary>Writes a value of an Obj's Mob Motion: false when the Obj has none.</summary>
	public static bool Set(Obj obj, int what, Vector4 value)
	{
		return Native.IsConnected && Native.Api->MobMotionValue(obj.Id, what, 1, (float*)&value) == 1;
	}

	public static float Number(Obj obj, int what, float missing)
	{
		return Get(obj, what, out var value) ? value.X : missing;
	}
}
