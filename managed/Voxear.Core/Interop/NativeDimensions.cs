using System;
using System.Numerics;

namespace Voxear.Interop;

/// <summary>
/// The engine's dimensions, as dimension_scripts.rs numbers what it reads and writes. A dimension travels as its
/// handle: its number, and above it a serial that tells it from a later dimension given the same number.
/// </summary>
internal static unsafe class NativeDimensions
{
	private const int OfObjValue = 1;
	private const int CurrentValue = 2;
	private const int MoveValue = 3;
	private const int KeepLoadedValue = 4;
	private const int ReadyValue = 5;
	private const int AskInValue = 6;
	private const int AskEndValue = 7;
	private const int IsMadeValue = 9;
	private const int SkyValue = 10;
	private const int ClockValue = 11;
	private const int WeatherValue = 12;
	private const int WeatherFirstValue = 13;
	private const int WeatherLastValue = 14;

	public const int MakeText = 1;
	public const int AskedText = 2;
	public const int InfoText = 3;
	public const int MadeText = 4;
	public const int StagesText = 5;
	public const int SetStagesText = 6;
	public const int SetStageText = 7;
	public const int LetGoText = 8;
	public const int ElsewhereText = 9;

	/// <summary>The handle of the dimension an Obj is in: 0, the level's, for one that is gone.</summary>
	public static uint Of(Obj obj)
	{
		return NativeComponent.Get(NativeComponent.Dimension, obj, OfObjValue, out var value) ? Handle(value.X) : 0;
	}

	/// <summary>The handle of the dimension the game is seen from.</summary>
	public static uint Current()
	{
		return NativeComponent.Get(NativeComponent.Dimension, Obj.None, CurrentValue, out var value) ? Handle(value.X) : 0;
	}

	/// <summary>Moves an Obj into a dimension made, at a place there. False where either is gone.</summary>
	public static bool Move(Obj obj, uint dimension, Vector3 position)
	{
		return NativeComponent.Set(NativeComponent.Dimension, obj, MoveValue, new Vector4(position, BitConverter.UInt32BitsToSingle(dimension)));
	}

	public static bool IsMade(uint dimension)
	{
		return Number(dimension, IsMadeValue) != 0f;
	}

	public static bool KeepLoaded(uint dimension)
	{
		return Number(dimension, KeepLoadedValue) != 0f;
	}

	public static void SetKeepLoaded(uint dimension, bool keep)
	{
		NativeComponent.Set(NativeComponent.Dimension, Carrier(dimension), KeepLoadedValue, new Vector4(keep ? 1f : 0f, 0f, 0f, 0f));
	}

	/// <summary>Whether a dimension's ground is made within <paramref name="radius"/> metres of a place.</summary>
	public static bool IsReady(uint dimension, Vector3 position, float radius)
	{
		NativeComponent.Set(NativeComponent.Dimension, Carrier(dimension), ReadyValue, new Vector4(position, radius));
		return Number(dimension, ReadyValue) != 0f;
	}

	/// <summary>Asks of a dimension's blocks and physics until <see cref="AskEnd"/>. False where it is not made.</summary>
	public static bool AskIn(uint dimension)
	{
		return NativeComponent.Set(NativeComponent.Dimension, Carrier(dimension), AskInValue, Vector4.Zero);
	}

	public static void AskEnd()
	{
		NativeComponent.Set(NativeComponent.Dimension, Obj.None, AskEndValue, Vector4.Zero);
	}

	public static Vector4 Sky(uint dimension)
	{
		return Value(dimension, SkyValue);
	}

	public static Vector4 Clock(uint dimension)
	{
		return Value(dimension, ClockValue);
	}

	public static Vector4 Weather(uint dimension)
	{
		return Value(dimension, WeatherValue);
	}

	/// <summary>What a dimension's weather adds now, in the order <see cref="WeatherAmounts"/> lists them.</summary>
	public static (Vector4 First, Vector4 Last) WeatherAmounts(uint dimension)
	{
		return (Value(dimension, WeatherFirstValue), Value(dimension, WeatherLastValue));
	}

	/// <summary>Asks something of the engine's dimensions with some text, and gives back its answer: null with none.</summary>
	public static string? Ask(int what, uint dimension, string text)
	{
		return NativeComponent.Ask(NativeComponent.Dimension, Carrier(dimension), what, text);
	}

	/// <summary>Tells the engine's dimensions something with some text: false where it was refused.</summary>
	public static bool Tell(int what, uint dimension, string text)
	{
		return NativeComponent.Tell(NativeComponent.Dimension, Carrier(dimension), what, text);
	}

	public static string? Text(int what, uint dimension)
	{
		return NativeComponent.Text(NativeComponent.Dimension, Carrier(dimension), what);
	}

	/// <summary>The changes of every dimension but the level's, for a save of the scene: empty where none has any.</summary>
	public static byte[] CaptureEdits()
	{
		if (!Native.IsConnected)
		{
			return [];
		}

		var length = Native.Api->BlockEdits(2, null, 0, null, 0);
		while (true)
		{
			var buffer = new byte[Math.Max(length, 0)];
			fixed (byte* into = buffer)
			{
				length = Native.Api->BlockEdits(2, null, 0, into, buffer.Length);
			}
			if (length <= buffer.Length)
			{
				return length == buffer.Length ? buffer : buffer[..length];
			}
		}
	}

	/// <summary>Puts back what <see cref="CaptureEdits"/> kept: false for bytes that are no dimensions' changes.</summary>
	public static bool ApplyEdits(byte[] edits)
	{
		if (!Native.IsConnected)
		{
			return false;
		}

		fixed (byte* from = edits)
		{
			return Native.Api->BlockEdits(3, from, edits.Length, null, 0) >= 0;
		}
	}

	/// <summary>A dimension's handle in the place of an Obj, as the engine reads it.</summary>
	private static Obj Carrier(uint dimension)
	{
		return new Obj(dimension);
	}

	private static uint Handle(float bits)
	{
		return BitConverter.SingleToUInt32Bits(bits);
	}

	private static float Number(uint dimension, int what)
	{
		return NativeComponent.Number(NativeComponent.Dimension, Carrier(dimension), what, 0f);
	}

	private static Vector4 Value(uint dimension, int what)
	{
		return NativeComponent.Get(NativeComponent.Dimension, Carrier(dimension), what, out var value) ? value : default;
	}
}
