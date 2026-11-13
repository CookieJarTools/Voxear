using System;
using System.Numerics;

namespace Voxear;

/// <summary>
/// A Block World's sea in rough weather: how it is drawn, how high its swell runs, and its whirlpools. The sea is
/// flat in calm weather and rises and falls under a storm; <see cref="Water"/> gives its height, slope and flow
/// there, and a Buoyancy rides it.
/// <code>
/// // A whirlpool where the boss dived, that pulls the boats in.
/// var pool = Sea.AddWhirlpool(boss.Transform.Position, 24f, 8f);
///
/// // Is the swimmer caught in one?
/// if (Sea.WhirlpoolAt(player.Transform.Position) &gt; 0.9f)
/// {
///     player.Drown();
/// }
/// </code>
/// </summary>
public static class Sea
{
	/// <summary>Raised when something floating is drawn down the middle of a whirlpool.</summary>
	public static event Action<Obj>? Swallowed;

	/// <summary>How the sea is drawn where it is rough.</summary>
	public static SeaLook Look
	{
		get => (SeaLook)(int)StormCalls.Rule(StormCalls.SeaLook);
		set => StormCalls.SetRule(StormCalls.SeaLook, (float)value);
	}

	/// <summary>How far the deepest trough of the roughest sea lies under a crest, in metres.</summary>
	public static float SwellHeight
	{
		get => StormCalls.Rule(StormCalls.Swell);
		set => StormCalls.SetRule(StormCalls.Swell, value);
	}

	/// <summary>Whether strong storms raise whirlpools over deep water.</summary>
	public static bool StormWhirlpools
	{
		get => StormCalls.Rule(StormCalls.Whirlpools) != 0f;
		set => StormCalls.SetRule(StormCalls.Whirlpools, value ? 1f : 0f);
	}

	/// <summary>Whether great whirlpools stand at places of their own in a generated sea.</summary>
	public static bool Maelstroms
	{
		get => StormCalls.Rule(StormCalls.Maelstroms) != 0f;
		set => StormCalls.SetRule(StormCalls.Maelstroms, value ? 1f : 0f);
	}

	/// <summary>How much warm seas glow at night where the water breaks, from 0 for none.</summary>
	public static float Glow
	{
		get => StormCalls.Rule(StormCalls.Glow);
		set => StormCalls.SetRule(StormCalls.Glow, value);
	}

	/// <summary>
	/// Makes a whirlpool over <paramref name="position"/>: its funnel <paramref name="radius"/> metres across from
	/// its middle to its rim, its middle drawn <paramref name="depth"/> metres down.
	/// </summary>
	public static Whirlpool AddWhirlpool(Vector3 position, float radius = 20f, float depth = 6f)
	{
		Span<float> numbers = stackalloc float[StormCalls.Numbers];
		numbers[0] = position.X;
		numbers[1] = position.Y;
		numbers[2] = position.Z;
		numbers[3] = radius;
		numbers[4] = depth;
		var id = StormCalls.Call(StormCalls.PoolAdd, 0, numbers);
		return id > 0 ? new Whirlpool((uint)id) : Whirlpool.None;
	}

	/// <summary>
	/// How far into a whirlpool <paramref name="position"/> is, from 0 at its rim and outside any to 1 at its
	/// middle, whoever made it.
	/// </summary>
	public static float WhirlpoolAt(Vector3 position)
	{
		Span<float> numbers = stackalloc float[StormCalls.Numbers];
		numbers[0] = position.X;
		numbers[1] = position.Y;
		numbers[2] = position.Z;
		return StormCalls.Call(StormCalls.PoolAt, 0, numbers) > 0 ? numbers[0] : 0f;
	}

	internal static void RaiseSwallowed(Obj obj)
	{
		Swallowed?.Invoke(obj);
	}
}
