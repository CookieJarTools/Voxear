using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Reflection Probe: inside the Obj's box, as its Transform places, turns and sizes it, surfaces reflect
/// what can be seen from the box's middle, rather than the sky. Get it with Obj.ReflectionProbe, or give an Obj one
/// with Obj.AddReflectionProbe.
/// <code>
/// // The hall's lights come on: what its floor reflects is captured again.
/// hallLights.Light.Intensity = 8f;
/// hall.ReflectionProbe.Capture();
/// </code>
/// </summary>
public readonly struct ReflectionProbe : IEquatable<ReflectionProbe>, IObjComponent
{
	private const int UpdateValue = 3;
	private const int IntervalValue = 4;
	private const int BlendDistanceValue = 5;
	private const int BoxProjectionValue = 6;
	private const int IntensityValue = 7;
	private const int ImportanceValue = 8;
	private const int CaptureValue = 9;

	public static readonly ReflectionProbe None = default;

	internal ReflectionProbe(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj whose box it is.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Reflection Probe, or is gone.</summary>
	public bool IsValid => Obj.IsValid && Number(NativeComponent.Has) != 0f;

	/// <summary>When it is captured: once, or live, every Interval seconds.</summary>
	public ReflectionProbeUpdate Update
	{
		get { return Number(UpdateValue) != 0f ? ReflectionProbeUpdate.Live : ReflectionProbeUpdate.Once; }
		set { Write(UpdateValue, value == ReflectionProbeUpdate.Live ? 1f : 0f); }
	}

	/// <summary>A live probe's seconds between captures; 0 captures it every frame.</summary>
	public float Interval
	{
		get { return Number(IntervalValue); }
		set { Write(IntervalValue, value); }
	}

	/// <summary>How far outside its box, in metres, it fades out into what lies round it; 0 for a hard edge.</summary>
	public float BlendDistance
	{
		get { return Number(BlendDistanceValue); }
		set { Write(BlendDistanceValue, value); }
	}

	/// <summary>
	/// Whether each reflection is bent to where it meets the box, so a room's probe lines up with its walls from
	/// anywhere in it. Best for a box the size of the room; off for a probe out in the open.
	/// </summary>
	public bool BoxProjection
	{
		get { return Number(BoxProjectionValue) != 0f; }
		set { Write(BoxProjectionValue, value ? 1f : 0f); }
	}

	/// <summary>How bright what it shows is, 1 as captured.</summary>
	public float Intensity
	{
		get { return Number(IntensityValue); }
		set { Write(IntensityValue, value); }
	}

	/// <summary>Where boxes overlap, the higher shows first; between equals, the smaller.</summary>
	public int Importance
	{
		get { return (int)Number(ImportanceValue); }
		set { Write(ImportanceValue, value); }
	}

	/// <summary>Captures it again, next frame or soon after, as after what it sees has changed.</summary>
	public void Capture()
	{
		Write(CaptureValue, 0f);
	}

	/// <summary>Takes the Reflection Probe off its Obj.</summary>
	public void Remove()
	{
		NativeComponent.Set(NativeComponent.ReflectionProbe, Obj, NativeComponent.Remove, Vector4.Zero);
	}

	public static bool operator ==(ReflectionProbe left, ReflectionProbe right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(ReflectionProbe left, ReflectionProbe right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(ReflectionProbe other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is ReflectionProbe probe && Equals(probe);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Reflection Probe";
	}

	private float Number(int what)
	{
		return NativeComponent.Number(NativeComponent.ReflectionProbe, Obj, what, 0f);
	}

	private void Write(int what, float value)
	{
		var written = NativeComponent.Set(NativeComponent.ReflectionProbe, Obj, what, new Vector4(value, 0f, 0f, 0f));
		if (!written && Native.IsConnected)
		{
			Log.Error($"{Obj} has no Reflection Probe; give it one with AddReflectionProbe.");
		}
	}
}
