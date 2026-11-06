using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Mist: a trail of fog it leaves behind as it moves, while Emitting. Each piece of the trail widens, rises
/// and thins away over Linger seconds, so stopped, what it left lingers and is gone. Get it with Obj.Mist, or give an
/// Obj one with Obj.AddMist.
/// <code>
/// // A dash in a cloud of mist: the body hidden, the trail left behind it.
/// Obj.Visible = false;
/// Obj.Mist.Emitting = true;
/// // ... the dash over:
/// Obj.Mist.Emitting = false;
/// Obj.Visible = true;
/// </code>
/// </summary>
public readonly struct Mist : IEquatable<Mist>, IObjComponent
{
	public static readonly Mist None = default;

	internal Mist(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj it trails behind.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Mist, or is gone.</summary>
	public bool IsValid => Obj.IsValid && NativeMist.Number(Obj, NativeMist.Has, 0f) != 0f;

	/// <summary>Whether it is leaving mist behind it now.</summary>
	public bool Emitting
	{
		get { return Number(NativeMist.Emitting) != 0f; }
		set { Set(NativeMist.Emitting, value ? 1f : 0f); }
	}

	/// <summary>How wide the trail is where it leaves its Obj, in metres; it widens to twice this as it thins.</summary>
	public float Width
	{
		get { return Number(NativeMist.Width); }
		set { Set(NativeMist.Width, value); }
	}

	/// <summary>How thick it is: how much of the light a metre of it takes.</summary>
	public float Density
	{
		get { return Number(NativeMist.Density); }
		set { Set(NativeMist.Density, value); }
	}

	public Color Color
	{
		get { return NativeMist.Get(Obj, NativeMist.Color, out var value) ? new Color(value.X, value.Y, value.Z) : Color.White; }
		set { Set(NativeMist.Color, new Vector4(value.R, value.G, value.B, 1f)); }
	}

	/// <summary>How long each piece of the trail lasts, in seconds, before it is gone.</summary>
	public float Linger
	{
		get { return Number(NativeMist.Linger); }
		set { Set(NativeMist.Linger, value); }
	}

	/// <summary>Which swirls: two mists alike but for this look different.</summary>
	public int Seed
	{
		get { return (int)Number(NativeMist.Seed); }
		set { Set(NativeMist.Seed, value); }
	}

	/// <summary>Takes the Mist off its Obj, and its trail with it.</summary>
	public void Remove()
	{
		NativeMist.Set(Obj, NativeMist.Remove, Vector4.Zero);
	}

	public static bool operator ==(Mist left, Mist right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(Mist left, Mist right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(Mist other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is Mist mist && Equals(mist);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Mist";
	}

	private float Number(int what)
	{
		return NativeMist.Number(Obj, what, 0f);
	}

	private void Set(int what, float value)
	{
		Set(what, new Vector4(value, 0f, 0f, 0f));
	}

	private void Set(int what, Vector4 value)
	{
		if (!NativeMist.Set(Obj, what, value) && Native.IsConnected)
		{
			Log.Error($"{Obj} has no Mist; give it one with AddMist.");
		}
	}
}
