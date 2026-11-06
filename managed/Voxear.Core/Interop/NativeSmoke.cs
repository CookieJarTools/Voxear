using System.Numerics;

namespace Voxear.Interop;

/// <summary>
/// The engine's Smokes, for Smoke: what each value's number is, as the engine's smoke.rs has them. Without the
/// engine, as in tests, no Obj has one, and every value reads as missing.
/// </summary>
internal static unsafe class NativeSmoke
{
	public const int Has = 0;
	public const int Add = 1;
	public const int Remove = 2;
	public const int Play = 3;
	public const int Disperse = 4;
	public const int Playing = 5;
	public const int Size = 6;
	public const int Cell = 7;
	public const int Density = 8;
	public const int Color = 9;
	public const int Spread = 10;
	public const int Lasts = 11;
	public const int Fade = 12;
	public const int Seed = 13;
	public const int PlayOnStart = 14;

	/// <summary>A value of an Obj's Smoke, or false when the Obj has none.</summary>
	public static bool Get(Obj obj, int what, out Vector4 value)
	{
		var read = default(Vector4);
		var found = Native.IsConnected && Native.Api->SmokeValue(obj.Id, what, 0, (float*)&read) == 1;
		value = read;
		return found;
	}

	/// <summary>Writes a value of an Obj's Smoke: false when the Obj has none.</summary>
	public static bool Set(Obj obj, int what, Vector4 value)
	{
		return Native.IsConnected && Native.Api->SmokeValue(obj.Id, what, 1, (float*)&value) == 1;
	}

	public static float Number(Obj obj, int what, float missing)
	{
		return Get(obj, what, out var value) ? value.X : missing;
	}
}
