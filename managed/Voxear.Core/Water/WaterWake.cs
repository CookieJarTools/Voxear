using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Water Wake: how it stirs the water it moves through. Characters and bodies with a Collider leave ripples,
/// a V of waves and foam behind them on any water with none, as wide as their bodies; a Water Wake makes the wake
/// wider or narrower, stronger or weaker, foamier or clear, gives an Obj with no body a wake of its own, and switched
/// off, stops it. Get it with Obj.WaterWake, or give an Obj one with Obj.AddWaterWake. Project Settings ▸ Graphics ▸
/// Water Trails says how finely, and how far round the camera, trails are worked out.
/// <code>
/// // A speedboat throws up more white water the faster it goes, and a ghost ship leaves none.
/// var wake = Obj.AddWaterWake();
/// wake.Foam = Mathf.Clamp(speed / 8f, 0.5f, 3f);
/// ghostShip.WaterWake.Enabled = false;
/// </code>
/// </summary>
public readonly struct WaterWake : IEquatable<WaterWake>, IObjComponent
{
	private const int SizeValue = 3;
	private const int StrengthValue = 4;
	private const int FoamValue = 5;
	private const int EnabledValue = 6;

	public static readonly WaterWake None = default;

	internal WaterWake(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj whose wake it is.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Water Wake, or is gone.</summary>
	public bool IsValid => Obj.IsValid && Number(NativeComponent.Has) != 0f;

	/// <summary>
	/// How wide it is where it cuts the water, in metres; 0 for as wide as its body, or half a metre with no body.
	/// </summary>
	public float Size
	{
		get { return Number(SizeValue); }
		set { Write(SizeValue, value); }
	}

	/// <summary>How hard it stirs the water: 1 as its body would, more for higher waves, 0 for none.</summary>
	public float Strength
	{
		get { return Number(StrengthValue); }
		set { Write(StrengthValue, value); }
	}

	/// <summary>How much foam it leaves: 1 as its body would, 0 for none.</summary>
	public float Foam
	{
		get { return Number(FoamValue); }
		set { Write(FoamValue, value); }
	}

	/// <summary>Whether it stirs the water at all; switched off, its Obj leaves no trail, even one its body would.</summary>
	public bool Enabled
	{
		get { return Number(EnabledValue) != 0f; }
		set { Write(EnabledValue, value ? 1f : 0f); }
	}

	/// <summary>Takes the Water Wake off its Obj, which then stirs the water as its body does, if it has one.</summary>
	public void Remove()
	{
		NativeComponent.Set(NativeComponent.WaterWake, Obj, NativeComponent.Remove, Vector4.Zero);
	}

	public static bool operator ==(WaterWake left, WaterWake right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(WaterWake left, WaterWake right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(WaterWake other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is WaterWake wake && Equals(wake);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Water Wake";
	}

	private float Number(int what)
	{
		return NativeComponent.Number(NativeComponent.WaterWake, Obj, what, 0f);
	}

	private void Write(int what, float value)
	{
		var written = NativeComponent.Set(NativeComponent.WaterWake, Obj, what, new Vector4(value, 0f, 0f, 0f));
		if (!written && Native.IsConnected)
		{
			Log.Error($"{Obj} has no Water Wake; give it one with AddWaterWake.");
		}
	}
}
