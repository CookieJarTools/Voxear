using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Heat Source: something hot or cold, as a campfire or a block of ice. At its middle it is Celsius, fading
/// to the place's own temperature by Radius metres away. Get it with Obj.HeatSource.
/// <code>
/// // The fire dies down as its wood burns.
/// campfire.HeatSource.Celsius = 200f + 400f * fuel;
/// </code>
/// </summary>
public readonly struct HeatSource : IEquatable<HeatSource>, IObjComponent
{
	private const int CelsiusValue = 3;
	private const int RadiusValue = 4;

	public static readonly HeatSource None = default;

	internal HeatSource(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj it belongs to.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Heat Source, or is gone.</summary>
	public bool IsValid => Obj.IsValid && NativeComponent.Number(NativeComponent.HeatSource, Obj, NativeComponent.Has, 0f) != 0f;

	/// <summary>How hot or cold it is at its middle, in °C.</summary>
	public float Celsius
	{
		get { return NativeComponent.Number(NativeComponent.HeatSource, Obj, CelsiusValue, 0f); }
		set { NativeComponent.Set(NativeComponent.HeatSource, Obj, CelsiusValue, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>How far it reaches, in metres.</summary>
	public float Radius
	{
		get { return NativeComponent.Number(NativeComponent.HeatSource, Obj, RadiusValue, 0f); }
		set { NativeComponent.Set(NativeComponent.HeatSource, Obj, RadiusValue, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>Takes the Heat Source off its Obj.</summary>
	public void Remove()
	{
		NativeComponent.Set(NativeComponent.HeatSource, Obj, NativeComponent.Remove, Vector4.Zero);
	}

	public static bool operator ==(HeatSource left, HeatSource right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(HeatSource left, HeatSource right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(HeatSource other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is HeatSource same && Equals(same);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Heat Source";
	}
}
