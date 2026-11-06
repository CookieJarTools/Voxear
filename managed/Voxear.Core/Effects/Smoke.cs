using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Smoke: a cloud that fills the space round it, spreading round walls and out through doors, as a smoke
/// grenade's does. Get it with Obj.Smoke, or give an Obj one with Obj.AddSmoke. It starts as its Obj appears, or with
/// PlayOnStart off, when Play is called. While the game runs, the cloud stays where it started, however its Obj
/// moves after.
/// <code>
/// // A smoke grenade: thrown, and after its fuse, smoking where it lies.
/// var grenade = smokeGrenade.Instantiate(hand.WorldPosition, hand.WorldRotation);
/// var body = grenade.Rigidbody;
/// body.Velocity = aim * 14f;
/// // ... after the fuse:
/// grenade.Smoke.Play();
/// </code>
/// </summary>
public readonly struct Smoke : IEquatable<Smoke>, IObjComponent
{
	public static readonly Smoke None = default;

	internal Smoke(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj it fills the space round.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Smoke, or is gone.</summary>
	public bool IsValid => Obj.IsValid && NativeSmoke.Number(Obj, NativeSmoke.Has, 0f) != 0f;

	/// <summary>Whether its cloud is there: from when it starts until it has dispersed.</summary>
	public bool IsPlaying => NativeSmoke.Number(Obj, NativeSmoke.Playing, 0f) != 0f;

	/// <summary>How much it fills: as much as a ball this many metres across holds.</summary>
	public float Size
	{
		get { return Number(NativeSmoke.Size); }
		set { Set(NativeSmoke.Size, value); }
	}

	/// <summary>How fine its cells are, in metres: finer follows walls more closely and reaches less far.</summary>
	public float CellSize
	{
		get { return Number(NativeSmoke.Cell); }
		set { Set(NativeSmoke.Cell, value); }
	}

	/// <summary>How thick it is: how much of the light a metre of it takes.</summary>
	public float Density
	{
		get { return Number(NativeSmoke.Density); }
		set { Set(NativeSmoke.Density, value); }
	}

	public Color Color
	{
		get { return NativeSmoke.Get(Obj, NativeSmoke.Color, out var value) ? new Color(value.X, value.Y, value.Z) : Color.White; }
		set { Set(NativeSmoke.Color, new Vector4(value.R, value.G, value.B, 1f)); }
	}

	/// <summary>How long it takes to spread out, in seconds.</summary>
	public float SpreadTime
	{
		get { return Number(NativeSmoke.Spread); }
		set { Set(NativeSmoke.Spread, value); }
	}

	/// <summary>How long it lasts before it starts to disperse, in seconds; 0 for ever.</summary>
	public float Lasts
	{
		get { return Number(NativeSmoke.Lasts); }
		set { Set(NativeSmoke.Lasts, value); }
	}

	/// <summary>How long it takes to disperse, in seconds.</summary>
	public float DisperseTime
	{
		get { return Number(NativeSmoke.Fade); }
		set { Set(NativeSmoke.Fade, value); }
	}

	/// <summary>Which swirls: two smokes alike but for this look different.</summary>
	public int Seed
	{
		get { return (int)Number(NativeSmoke.Seed); }
		set { Set(NativeSmoke.Seed, value); }
	}

	/// <summary>Whether it starts as its Obj appears. Off, it waits for Play, as a grenade's does for its fuse.</summary>
	public bool PlayOnStart
	{
		get { return Number(NativeSmoke.PlayOnStart) != 0f; }
		set { Set(NativeSmoke.PlayOnStart, value ? 1f : 0f); }
	}

	/// <summary>Starts it where its Obj is now, spreading out from there; over again if it had started.</summary>
	public void Play()
	{
		Set(NativeSmoke.Play, 0f);
	}

	/// <summary>Starts it dispersing now, over its DisperseTime, however long it was to last.</summary>
	public void Disperse()
	{
		Set(NativeSmoke.Disperse, 0f);
	}

	/// <summary>Takes the Smoke off its Obj, and its cloud with it.</summary>
	public void Remove()
	{
		NativeSmoke.Set(Obj, NativeSmoke.Remove, Vector4.Zero);
	}

	public static bool operator ==(Smoke left, Smoke right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(Smoke left, Smoke right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(Smoke other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is Smoke smoke && Equals(smoke);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Smoke";
	}

	private float Number(int what)
	{
		return NativeSmoke.Number(Obj, what, 0f);
	}

	private void Set(int what, float value)
	{
		Set(what, new Vector4(value, 0f, 0f, 0f));
	}

	private void Set(int what, Vector4 value)
	{
		if (!NativeSmoke.Set(Obj, what, value) && Native.IsConnected)
		{
			Log.Error($"{Obj} has no Smoke; give it one with AddSmoke.");
		}
	}
}
