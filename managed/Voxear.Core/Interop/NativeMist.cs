using System.Numerics;

namespace Voxear.Interop;

/// <summary>
/// The engine's Mists, for Mist: what each value's number is, as the engine's mist.rs has them. Without the engine,
/// as in tests, no Obj has one, and every value reads as missing.
/// </summary>
internal static unsafe class NativeMist
{
	public const int Has = 0;
	public const int Add = 1;
	public const int Remove = 2;
	public const int Emitting = 3;
	public const int Width = 4;
	public const int Density = 5;
	public const int Color = 6;
	public const int Linger = 7;
	public const int Seed = 8;

	/// <summary>A value of an Obj's Mist, or false when the Obj has none.</summary>
	public static bool Get(Obj obj, int what, out Vector4 value)
	{
		var read = default(Vector4);
		var found = Native.IsConnected && Native.Api->MistValue(obj.Id, what, 0, (float*)&read) == 1;
		value = read;
		return found;
	}

	/// <summary>Writes a value of an Obj's Mist: false when the Obj has none.</summary>
	public static bool Set(Obj obj, int what, Vector4 value)
	{
		return Native.IsConnected && Native.Api->MistValue(obj.Id, what, 1, (float*)&value) == 1;
	}

	public static float Number(Obj obj, int what, float missing)
	{
		return Get(obj, what, out var value) ? value.X : missing;
	}
}
