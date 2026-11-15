using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Reverb Probe: a box round the Obj, Size metres along each axis of the world, filled with probes Spacing
/// metres apart, each baked from the editor with how long sound rings there. With ray-traced audio off, the room the
/// listener is in rings as the probes round it say. Get it with Obj.ReverbProbe, or give an Obj one with
/// Obj.AddReverbProbe; bake it from the level's Inspector.
/// <code>
/// var cave = Obj.Find("Cave").AddReverbProbe();
/// cave.Size = new Vector3(40f, 12f, 30f);
/// cave.Spacing = 4f;
/// </code>
/// </summary>
public readonly struct ReverbProbe : IEquatable<ReverbProbe>, IObjComponent
{
	private const int SizeValue = 3;
	private const int SpacingValue = 4;

	public static readonly ReverbProbe None = default;

	internal ReverbProbe(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj whose box it is.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Reverb Probe, or is gone.</summary>
	public bool IsValid => Obj.IsValid && NativeComponent.Number(NativeComponent.ReverbProbe, Obj, NativeComponent.Has, 0f) != 0f;

	/// <summary>How far the box reaches along each axis of the world, in metres, round the Obj.</summary>
	public Vector3 Size
	{
		get
		{
			NativeComponent.Get(NativeComponent.ReverbProbe, Obj, SizeValue, out var size);
			return new Vector3(size.X, size.Y, size.Z);
		}
		set { Write(SizeValue, new Vector4(value, 0f)); }
	}

	/// <summary>How far apart its probes are, in metres: closer hears small rooms better, and bakes longer.</summary>
	public float Spacing
	{
		get { return NativeComponent.Number(NativeComponent.ReverbProbe, Obj, SpacingValue, 0f); }
		set { Write(SpacingValue, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>Takes the Reverb Probe off its Obj.</summary>
	public void Remove()
	{
		NativeComponent.Set(NativeComponent.ReverbProbe, Obj, NativeComponent.Remove, Vector4.Zero);
	}

	public static bool operator ==(ReverbProbe left, ReverbProbe right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(ReverbProbe left, ReverbProbe right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(ReverbProbe other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is ReverbProbe probe && Equals(probe);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Reverb Probe";
	}

	private void Write(int what, Vector4 value)
	{
		var written = NativeComponent.Set(NativeComponent.ReverbProbe, Obj, what, value);
		if (!written && Native.IsConnected)
		{
			Log.Error($"{Obj} has no Reverb Probe; give it one with AddReverbProbe.");
		}
	}
}
