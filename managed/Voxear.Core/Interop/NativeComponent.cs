using System.Numerics;

namespace Voxear.Interop;

/// <summary>
/// The engine's components reached by kind through one entry, as the engine's interop.rs numbers them in
/// ComponentKind. Without the engine, or in a game without a kind's module, no Obj has one.
/// </summary>
internal static unsafe class NativeComponent
{
	public const int Camera = 1;
	public const int FirstPersonView = 2;
	public const int LimbIK = 3;
	public const int Gun = 16;
	public const int GunHolder = 17;

	/// <summary>The value every kind reads as 1 when the Obj has one.</summary>
	public const int Has = 0;

	/// <summary>The value every kind adds one by when written.</summary>
	public const int Add = 1;

	/// <summary>The value every kind takes its one away by when written.</summary>
	public const int Remove = 2;

	/// <summary>A value of an Obj's component of a kind, or false when the Obj has none.</summary>
	public static bool Get(int kind, Obj obj, int what, out Vector4 value)
	{
		var read = default(Vector4);
		var found = Native.IsConnected && Native.Api->KindValue(kind, obj.Id, what, 0, (float*)&read) == 1;
		value = read;
		return found;
	}

	/// <summary>A number of an Obj's component of a kind, or <paramref name="otherwise"/> when it has none.</summary>
	public static float Number(int kind, Obj obj, int what, float otherwise)
	{
		return Get(kind, obj, what, out var value) ? value.X : otherwise;
	}

	/// <summary>Writes a value of an Obj's component of a kind: false when the Obj has none.</summary>
	public static bool Set(int kind, Obj obj, int what, Vector4 value)
	{
		return Native.IsConnected && Native.Api->KindValue(kind, obj.Id, what, 1, (float*)&value) == 1;
	}
}
