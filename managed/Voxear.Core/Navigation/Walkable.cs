using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Walkable: its still collider is walked on in a world of blocks, its floors and stairs stood on, its walls
/// gone round and its doorways walked through, worked out again as the Obj is placed or moved. Mark a door or a gate
/// that opens as a Door, and it is walked through. Get it with Obj.Walkable, or give an Obj one with Obj.AddWalkable.
/// </summary>
public sealed class Walkable : WholeComponent
{
	internal Walkable(Obj obj)
		: base(obj, NativeWhole.Whole, NativeWhole.Walkable)
	{
	}

	/// <summary>Whether it is a door or a gate that opens: walked through, not round.</summary>
	public bool Door
	{
		get => Switch("door", false);
		set => Change("door", value);
	}

	internal static Walkable? Of(Obj obj)
	{
		var walkable = new Walkable(obj);
		return walkable.IsValid ? walkable : null;
	}

	public override string ToString()
	{
		return $"{Obj}'s Walkable";
	}
}
