using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Placement, on a prefab's root: how much ground it takes up, which way its front faces and where its door
/// is, so Placing lines it up along a road facing it, scatters it, and keeps others clear of it. Get it with
/// Obj.Placement, or give an Obj one with Obj.AddPlacement.
/// </summary>
public sealed class Placement : WholeComponent
{
	internal Placement(Obj obj)
		: base(obj, NativeWhole.Whole, NativeWhole.Placement)
	{
	}

	/// <summary>How much ground it takes up across and along its front, in metres.</summary>
	public Vector2 Size
	{
		get => Two("size", new Vector2(6f, 6f));
		set => Change("size", PostJson.Of(value));
	}

	/// <summary>Its footprint's middle from the Obj along x and z, in metres.</summary>
	public Vector2 Centre
	{
		get => Two("centre", Vector2.Zero);
		set => Change("centre", PostJson.Of(value));
	}

	/// <summary>Which way its front faces, in degrees about up from +z.</summary>
	public float Front
	{
		get => Number("front", 0f);
		set => Change("front", value);
	}

	/// <summary>Where its door is, in its own space.</summary>
	public Vector3 Door
	{
		get => Three("door", new Vector3(0f, 0f, 3f));
		set => Change("door", PostJson.Of(value));
	}

	internal static Placement? Of(Obj obj)
	{
		var placement = new Placement(obj);
		return placement.IsValid ? placement : null;
	}

	public override string ToString()
	{
		return $"{Obj}'s Placement";
	}
}
