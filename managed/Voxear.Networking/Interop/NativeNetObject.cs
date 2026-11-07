using System.Numerics;

namespace Voxear.Interop;

/// <summary>
/// The engine's Net Objects, for NetObject: what each value's number is, as the engine's net/scripts.rs has them.
/// Without the engine, as in tests, no Obj has one.
/// </summary>
internal static unsafe class NativeNetObject
{
	public const int Has = 0;
	public const int Add = 1;
	public const int Remove = 2;
	public const int Priority = 3;
	public const int Range = 4;
	public const int Id = 5;
	public const int Owner = 6;
	public const int HasAuthority = 7;
	public const int Claim = 8;

	public static bool Get(Obj obj, int what, out Vector4 value)
	{
		var read = default(Vector4);
		var found = Native.IsConnected && Native.Api->NetObjectValue(obj.Id, what, 0, (float*)&read) == 1;
		value = read;
		return found;
	}

	public static bool Set(Obj obj, int what, Vector4 value)
	{
		return Native.IsConnected && Native.Api->NetObjectValue(obj.Id, what, 1, (float*)&value) == 1;
	}

	public static float Number(Obj obj, int what, float missing)
	{
		return Get(obj, what, out var value) ? value.X : missing;
	}
}
