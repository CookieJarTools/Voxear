using System.Numerics;
using System.Text;

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
	public const int LegTurn = 13;
	public const int FootLocking = 14;
	public const int AimPitch = 15;
	public const int StateRole = 16;
	public const int RootMotion = 17;
	public const int ActionTime = 18;
	public const int ClipVelocity = 19;
	public const int ActionTakesMovement = 20;
	public const int FeetDown = 21;

	public const int MotionSetText = 0;
	public const int SetMotionSet = 1;
	public const int Play = 2;
	public const int Stop = 3;
	public const int StateName = 4;
	public const int SetState = 5;
	public const int ActionName = 6;

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

	/// <summary>Asks the matcher with some text, as to play an action by its name: false when the Obj has none.</summary>
	public static bool Tell(Obj obj, int what, string text)
	{
		if (!Native.IsConnected)
		{
			return false;
		}
		var bytes = Encoding.UTF8.GetBytes(text);
		fixed (byte* written = bytes)
		{
			return Native.Api->MotionMatcherText(obj.Id, what, written, bytes.Length, null, 0) >= 0;
		}
	}

	/// <summary>A text of the matcher, as the name of the action it plays: null with none, or no matcher.</summary>
	public static string? Text(Obj obj, int what)
	{
		if (!Native.IsConnected)
		{
			return null;
		}
		var length = Native.Api->MotionMatcherText(obj.Id, what, null, 0, null, 0);
		if (length < 0)
		{
			return null;
		}
		var bytes = new byte[length];
		fixed (byte* buffer = bytes)
		{
			Native.Api->MotionMatcherText(obj.Id, what, null, 0, buffer, length);
		}
		return Encoding.UTF8.GetString(bytes);
	}
}
