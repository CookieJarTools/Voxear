using System.Numerics;

namespace Voxear.Interop;

/// <summary>
/// The engine's Block Mobs, for BlockMob: what each value's number is, as the engine's block_mob.rs has them.
/// Without the engine, as in tests, no Obj has one, and every value reads as missing.
/// </summary>
internal static unsafe class NativeBlockMob
{
	public const int Has = 0;
	public const int Add = 1;
	public const int Remove = 2;
	public const int MoveTo = 3;
	public const int Follow = 4;
	public const int Wander = 5;
	public const int Stop = 6;
	public const int Arrived = 7;
	public const int Speed = 8;
	public const int TurnSpeed = 9;
	public const int Swims = 10;
	public const int Goal = 11;

	/// <summary>A value of an Obj's Block Mob, or false when the Obj has none.</summary>
	public static bool Get(Obj obj, int what, out Vector4 value)
	{
		var read = default(Vector4);
		var found = Native.IsConnected && Native.Api->BlockMobValue(obj.Id, what, 0, (float*)&read) == 1;
		value = read;
		return found;
	}

	/// <summary>Writes a value of an Obj's Block Mob: false when the Obj has none.</summary>
	public static bool Set(Obj obj, int what, Vector4 value)
	{
		return Native.IsConnected && Native.Api->BlockMobValue(obj.Id, what, 1, (float*)&value) == 1;
	}

	public static float Number(Obj obj, int what, float missing)
	{
		return Get(obj, what, out var value) ? value.X : missing;
	}
}
