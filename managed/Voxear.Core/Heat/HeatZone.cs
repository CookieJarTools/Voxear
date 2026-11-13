using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Heat Zone: an area hotter or colder than round it, as a forge's room or a cold store. A box Size metres
/// across, or a sphere Size.X across, that sets the temperature in it to Celsius or adds Celsius to it, coming on
/// over Edge metres inside. Get it with Obj.HeatZone.
/// <code>
/// var store = cellar.AddHeatZone();
/// store.Celsius = 2f;
/// store.Size = new Vector3(6f, 3f, 8f);
/// </code>
/// </summary>
public readonly struct HeatZone : IEquatable<HeatZone>, IObjComponent
{
	private const int CelsiusValue = 3;
	private const int ShapeValue = 4;
	private const int SizeValue = 5;
	private const int ModeValue = 6;
	private const int EdgeValue = 7;

	public static readonly HeatZone None = default;

	internal HeatZone(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj it belongs to.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Heat Zone, or is gone.</summary>
	public bool IsValid => Obj.IsValid && Read(NativeComponent.Has) != 0f;

	/// <summary>The temperature it sets, or adds, in °C.</summary>
	public float Celsius
	{
		get { return Read(CelsiusValue); }
		set { Write(CelsiusValue, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>Whether it is a box or a sphere.</summary>
	public HeatZoneShape Shape
	{
		get { return (HeatZoneShape)(int)Read(ShapeValue); }
		set { Write(ShapeValue, new Vector4((int)value, 0f, 0f, 0f)); }
	}

	/// <summary>How large it is, in metres: a box's sides, or a sphere's width in X.</summary>
	public Vector3 Size
	{
		get { return NativeComponent.Get(NativeComponent.HeatZone, Obj, SizeValue, out var value) ? new Vector3(value.X, value.Y, value.Z) : Vector3.Zero; }
		set { Write(SizeValue, new Vector4(value, 0f)); }
	}

	/// <summary>Whether it sets the temperature in it or adds to it.</summary>
	public HeatZoneMode Mode
	{
		get { return (HeatZoneMode)(int)Read(ModeValue); }
		set { Write(ModeValue, new Vector4((int)value, 0f, 0f, 0f)); }
	}

	/// <summary>How far inside it it comes on, in metres; 0 all at once at its side.</summary>
	public float Edge
	{
		get { return Read(EdgeValue); }
		set { Write(EdgeValue, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>Takes the Heat Zone off its Obj.</summary>
	public void Remove()
	{
		Write(NativeComponent.Remove, Vector4.Zero);
	}

	private float Read(int what)
	{
		return NativeComponent.Number(NativeComponent.HeatZone, Obj, what, 0f);
	}

	private void Write(int what, Vector4 value)
	{
		NativeComponent.Set(NativeComponent.HeatZone, Obj, what, value);
	}

	public static bool operator ==(HeatZone left, HeatZone right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(HeatZone left, HeatZone right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(HeatZone other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is HeatZone same && Equals(same);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Heat Zone";
	}
}
