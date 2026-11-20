using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Nav Link: a way that joins two places walking does not, by a ladder, a gap, a drop or a door that takes
/// you elsewhere, from its Obj to its End. Get it with Obj.NavLink, null when the Obj has none, or give an Obj one with
/// Obj.AddNavLink.
/// <code>
/// // A drawbridge: the way over is there only while it is down.
/// Obj.NavLink?.Cost = lowered ? 1f : 1000f;
/// </code>
/// </summary>
public sealed class NavLink : WholeComponent
{
	internal NavLink(Obj obj)
		: base(obj, NativeWhole.Whole, NativeWhole.NavLink)
	{
	}

	/// <summary>Its far end, from its Obj, in metres in the Obj's space.</summary>
	public Vector3 End
	{
		get => Three("end", new Vector3(0f, 0f, 2f));
		set => Change("end", PostJson.Of(value));
	}

	/// <summary>How it is crossed.</summary>
	public NavLinkKind Kind
	{
		get => Choice("kind", NavLinkKind.Climb);
		set => Change("kind", PostJson.Of(value));
	}

	/// <summary>Whether it is crossed both ways, or only from its Obj to its End.</summary>
	public bool BothWays
	{
		get => Switch("both_ways", true);
		set => Change("both_ways", value);
	}

	/// <summary>How much crossing it costs over how far it goes, as so many metres more.</summary>
	public float Cost
	{
		get => Number("cost", 1f);
		set => Change("cost", value);
	}

	internal static NavLink? Of(Obj obj)
	{
		var link = new NavLink(obj);
		return link.IsValid ? link : null;
	}

	public override string ToString()
	{
		return $"{Obj}'s Nav Link";
	}
}
