using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Nav Area: a box of the world, turned with its Obj, whose ground navigation counts as another area, a road
/// sought out or a field kept off. Get it with Obj.NavArea, null when the Obj has none, or give an Obj one with
/// Obj.AddNavArea. Ways searched after a change go by it.
/// <code>
/// // A bridge that burns: nothing walks over it any more.
/// Obj.NavArea?.Mark = NavAreaMark.NotWalkable;
/// </code>
/// </summary>
public sealed class NavArea : WholeComponent
{
	internal NavArea(Obj obj)
		: base(obj, NativeWhole.Whole, NativeWhole.NavArea)
	{
	}

	/// <summary>The box's size, in metres.</summary>
	public Vector3 Size
	{
		get => Three("size", new Vector3(4f, 2f, 4f));
		set => Change("size", PostJson.Of(value));
	}

	/// <summary>What it marks the ground in its box as.</summary>
	public NavAreaMark Mark
	{
		get => Choice("area", NavAreaMark.Road);
		set => Change("area", PostJson.Of(value));
	}

	internal static NavArea? Of(Obj obj)
	{
		var area = new NavArea(obj);
		return area.IsValid ? area : null;
	}

	public override string ToString()
	{
		return $"{Obj}'s Nav Area";
	}
}
