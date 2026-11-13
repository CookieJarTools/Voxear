using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>What Storms, Storm, Sea and Whirlpool ask of the engine, as voxear_engine::storms::scripts numbers it.</summary>
internal static unsafe class StormCalls
{
	public const int Numbers = 16;

	public const int Start = 0;
	public const int Get = 1;
	public const int SetPlace = 2;
	public const int SetRadius = 3;
	public const int SetStrength = 4;
	public const int SetKind = 5;
	public const int SetVelocity = 6;
	public const int SetEye = 7;
	public const int SetLife = 8;
	public const int Stop = 9;
	public const int PathClear = 10;
	public const int PathAdd = 11;
	public const int Count = 12;
	public const int IdAt = 13;
	public const int WeatherAt = 14;
	public const int Event = 15;
	public const int PoolAdd = 16;
	public const int PoolRemove = 17;
	public const int PoolAt = 18;
	public const int RuleGet = 19;
	public const int RuleSet = 20;
	public const int Clock = 21;
	public const int SetClock = 22;
	public const int StrengthAt = 23;

	public const uint Natural = 0;
	public const uint Frequency = 1;
	public const uint Largest = 2;
	public const uint Standing = 3;
	public const uint SeaLook = 4;
	public const uint Swell = 5;
	public const uint Whirlpools = 6;
	public const uint Maelstroms = 7;
	public const uint Glow = 8;

	/// <summary>Asks <paramref name="what"/> of <paramref name="id"/> with the numbers given, which the answer overwrites.</summary>
	public static int Call(int what, uint id, Span<float> numbers)
	{
		if (!Native.IsConnected)
		{
			return -1;
		}
		fixed (float* given = numbers)
		{
			return Native.Api->StormCall(what, id, given);
		}
	}

	/// <summary>Asks <paramref name="what"/> of <paramref name="id"/> with up to four numbers.</summary>
	public static int Call(int what, uint id, float a = 0f, float b = 0f, float c = 0f, float d = 0f)
	{
		Span<float> numbers = stackalloc float[Numbers];
		numbers[0] = a;
		numbers[1] = b;
		numbers[2] = c;
		numbers[3] = d;
		return Call(what, id, numbers);
	}

	/// <summary>One number the engine gives for <paramref name="what"/> of <paramref name="id"/>, from place <paramref name="at"/>.</summary>
	public static float Read(int what, uint id, int at, float otherwise = 0f)
	{
		Span<float> numbers = stackalloc float[Numbers];
		return Call(what, id, numbers) > 0 ? numbers[at] : otherwise;
	}

	public static float Rule(uint which)
	{
		return Read(RuleGet, which, 0);
	}

	public static void SetRule(uint which, float value)
	{
		Call(RuleSet, which, value);
	}

	public static Vector3 Place(Span<float> numbers)
	{
		return new Vector3(numbers[0], numbers[1], numbers[2]);
	}
}
