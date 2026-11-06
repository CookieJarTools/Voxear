using System.Numerics;

namespace Voxear.Interop;

/// <summary>
/// The engine's Motion Matchers, for MotionMatcher: what each value's number is, as the engine's motion_matching.rs
/// has them. Without the engine, as in tests, no Obj has one, and every value reads as missing.
/// </summary>
internal static unsafe class NativeMotionMatcher
{
	public const int Has = 0;
	public const int Add = 1;
	public const int Remove = 2;
	public const int WantedVelocity = 3;
	public const int WantedFacing = 4;
	public const int Follow = 5;
	public const int IdleSpeed = 6;
	public const int SearchEvery = 7;
	public const int Blend = 8;
	public const int TurnRate = 9;
	public const int TurnToMovement = 10;
	public const int MatchSpeed = 11;
	public const int Debug = 12;

	/// <summary>A value of an Obj's Motion Matcher, or false when the Obj has none.</summary>
	public static bool Get(Obj obj, int what, out Vector4 value)
	{
		var read = default(Vector4);
		var found = Native.IsConnected && Native.Api->MotionMatcherValue(obj.Id, what, 0, (float*)&read) == 1;
		value = read;
		return found;
	}

	/// <summary>Writes a value of an Obj's Motion Matcher: false when the Obj has none.</summary>
	public static bool Set(Obj obj, int what, Vector4 value)
	{
		return Native.IsConnected && Native.Api->MotionMatcherValue(obj.Id, what, 1, (float*)&value) == 1;
	}

	public static float Number(Obj obj, int what, float missing)
	{
		return Get(obj, what, out var value) ? value.X : missing;
	}
}
