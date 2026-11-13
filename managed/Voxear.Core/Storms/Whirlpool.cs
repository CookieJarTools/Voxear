using System;

namespace Voxear;

/// <summary>A whirlpool a script made with <see cref="Sea.AddWhirlpool"/>.</summary>
public readonly struct Whirlpool : IEquatable<Whirlpool>
{
	/// <summary>No whirlpool.</summary>
	public static readonly Whirlpool None = default;

	private readonly uint id;

	internal Whirlpool(uint id)
	{
		this.id = id;
	}

	/// <summary>Whether it was made.</summary>
	public bool IsValid => id != 0;

	/// <summary>Takes it out of the sea. False when it was already gone.</summary>
	public bool Remove()
	{
		return id != 0 && StormCalls.Call(StormCalls.PoolRemove, id) > 0;
	}

	public static bool operator ==(Whirlpool left, Whirlpool right)
	{
		return left.id == right.id;
	}

	public static bool operator !=(Whirlpool left, Whirlpool right)
	{
		return left.id != right.id;
	}

	public bool Equals(Whirlpool other)
	{
		return id == other.id;
	}

	public override bool Equals(object? obj)
	{
		return obj is Whirlpool other && Equals(other);
	}

	public override int GetHashCode()
	{
		return (int)id;
	}
}
