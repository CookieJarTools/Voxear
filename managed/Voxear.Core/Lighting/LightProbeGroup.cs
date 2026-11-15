using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Light Probe Group: light probes, points Bake Lighting bakes the light reaching from every way, which
/// light every Obj without a lightmap, blended for where it is. Probes are placed by hand, in the Obj's own space,
/// and with a Fill, as a grid filling the Obj's box. Get it with Obj.LightProbeGroup, or give an Obj one with
/// Obj.AddLightProbeGroup.
/// <code>
/// var probes = hall.AddLightProbeGroup();
/// probes.Fill = 0f;                        // by hand only
/// probes.AddPoint(new Vector3(0f, 1.5f, 0f));
/// </code>
/// </summary>
public readonly struct LightProbeGroup : IEquatable<LightProbeGroup>, IObjComponent
{
	private const int FillValue = 3;
	private const int PointCountValue = 4;
	private const int AddPointValue = 5;
	private const int ClearPointsValue = 6;
	private const int ProbeCountValue = 7;
	private const int PointValue = 100;

	public static readonly LightProbeGroup None = default;

	internal LightProbeGroup(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj whose probes they are.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Light Probe Group, or is gone.</summary>
	public bool IsValid => Obj.IsValid && Number(NativeComponent.Has) != 0f;

	/// <summary>How far apart, in metres, the grid filling the Obj's box is; 0 for no grid, only the points.</summary>
	public float Fill
	{
		get { return Number(FillValue); }
		set { Write(FillValue, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>How many probes it places, its points and its grid's.</summary>
	public int ProbeCount => (int)Number(ProbeCountValue);

	/// <summary>How many probes are placed by hand.</summary>
	public int PointCount => (int)Number(PointCountValue);

	/// <summary>A probe placed by hand, where it is in the Obj's own space.</summary>
	public Vector3 this[int index]
	{
		get
		{
			var found = NativeComponent.Get(NativeComponent.LightProbeGroup, Obj, PointValue + index, out var value);
			return found ? new Vector3(value.X, value.Y, value.Z) : Vector3.Zero;
		}
		set { Write(PointValue + index, new Vector4(value, 0f)); }
	}

	/// <summary>Places a probe by hand, in the Obj's own space.</summary>
	public void AddPoint(Vector3 point)
	{
		Write(AddPointValue, new Vector4(point, 0f));
	}

	/// <summary>Takes away every probe placed by hand.</summary>
	public void ClearPoints()
	{
		Write(ClearPointsValue, Vector4.Zero);
	}

	/// <summary>Takes the Light Probe Group off its Obj.</summary>
	public void Remove()
	{
		NativeComponent.Set(NativeComponent.LightProbeGroup, Obj, NativeComponent.Remove, Vector4.Zero);
	}

	public static bool operator ==(LightProbeGroup left, LightProbeGroup right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(LightProbeGroup left, LightProbeGroup right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(LightProbeGroup other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is LightProbeGroup group && Equals(group);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Light Probe Group";
	}

	private float Number(int what)
	{
		return NativeComponent.Number(NativeComponent.LightProbeGroup, Obj, what, 0f);
	}

	private void Write(int what, Vector4 value)
	{
		var written = NativeComponent.Set(NativeComponent.LightProbeGroup, Obj, what, value);
		if (!written && Native.IsConnected)
		{
			Log.Error($"{Obj} has no Light Probe Group; give it one with AddLightProbeGroup.");
		}
	}
}
