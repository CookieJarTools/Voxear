using System.Numerics;

namespace Voxear.Interop;

/// <summary>
/// The engine's lights, for Light: what each value's number is, as the engine's lights.rs has them. Without the
/// engine, as in tests, no Obj has a Light, and every value reads as missing.
/// </summary>
internal static unsafe class NativeLights
{
	public const int Has = 0;
	public const int Add = 1;
	public const int Remove = 2;
	public const int Kind = 3;
	public const int Color = 4;
	public const int Intensity = 5;
	public const int Range = 6;
	public const int SpotAngles = 7;
	public const int Temperature = 8;
	public const int Shadows = 9;
	public const int Size = 10;
	public const int Radius = 11;
	public const int Length = 12;
	public const int Softness = 13;

	/// <summary>A value of an Obj's Light, or false when the Obj has none.</summary>
	public static bool Get(Obj obj, int what, out Vector4 value)
	{
		var read = default(Vector4);
		var found = Native.IsConnected && Native.Api->LightValue(obj.Id, what, 0, (float*)&read) == 1;
		value = read;
		return found;
	}

	/// <summary>Writes a value of an Obj's Light: false when the Obj has none.</summary>
	public static bool Set(Obj obj, int what, Vector4 value)
	{
		return Native.IsConnected && Native.Api->LightValue(obj.Id, what, 1, (float*)&value) == 1;
	}

	public static float Number(Obj obj, int what, float missing)
	{
		return Get(obj, what, out var value) ? value.X : missing;
	}
}
