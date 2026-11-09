using System;
using System.Numerics;
using System.Text;

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
	public const int UiPanel = 4;
	public const int Skeleton = 5;
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

	/// <summary>An Obj as a value: its id's two halves, bit for bit.</summary>
	public static Vector4 ValueOf(Obj obj)
	{
		var id = obj.Id;
		return new Vector4(BitConverter.UInt32BitsToSingle((uint)id), BitConverter.UInt32BitsToSingle((uint)(id >> 32)), 0f, 0f);
	}

	/// <summary>The Obj a value names, or None.</summary>
	public static Obj ObjOf(Vector4 value)
	{
		var id = BitConverter.SingleToUInt32Bits(value.X) | (ulong)BitConverter.SingleToUInt32Bits(value.Y) << 32;
		return id == 0 ? Obj.None : new Obj(id);
	}

	/// <summary>Writes a value of an Obj's component of a kind: false when the Obj has none.</summary>
	public static bool Set(int kind, Obj obj, int what, Vector4 value)
	{
		return Native.IsConnected && Native.Api->KindValue(kind, obj.Id, what, 1, (float*)&value) == 1;
	}

	/// <summary>Asks an Obj's component of a kind something with some text: false when the Obj has none.</summary>
	public static bool Tell(int kind, Obj obj, int what, string text)
	{
		if (!Native.IsConnected)
		{
			return false;
		}
		var bytes = Encoding.UTF8.GetBytes(text);
		fixed (byte* written = bytes)
		{
			return Native.Api->KindText(kind, obj.Id, what, written, bytes.Length, null, 0) >= 0;
		}
	}

	/// <summary>Asks an Obj's component of a kind something with some text, and gives back its answer: null with none.</summary>
	public static string? Ask(int kind, Obj obj, int what, string text)
	{
		if (!Native.IsConnected)
		{
			return null;
		}
		var bytes = Encoding.UTF8.GetBytes(text);
		fixed (byte* written = bytes)
		{
			var length = Native.Api->KindText(kind, obj.Id, what, written, bytes.Length, null, 0);
			if (length < 0)
			{
				return null;
			}
			var answer = new byte[length];
			fixed (byte* buffer = answer)
			{
				Native.Api->KindText(kind, obj.Id, what, written, bytes.Length, buffer, length);
			}
			return Encoding.UTF8.GetString(answer);
		}
	}

	/// <summary>A text of an Obj's component of a kind: null with none, or no component.</summary>
	public static string? Text(int kind, Obj obj, int what)
	{
		if (!Native.IsConnected)
		{
			return null;
		}
		var length = Native.Api->KindText(kind, obj.Id, what, null, 0, null, 0);
		if (length < 0)
		{
			return null;
		}
		var bytes = new byte[length];
		fixed (byte* buffer = bytes)
		{
			Native.Api->KindText(kind, obj.Id, what, null, 0, buffer, length);
		}
		return Encoding.UTF8.GetString(bytes);
	}
}
