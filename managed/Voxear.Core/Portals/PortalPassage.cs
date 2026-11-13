using System;
using System.Numerics;

namespace Voxear;

/// <summary>
/// An Obj going through a Portal, as Portal.Passed tells it: which Obj, from which Portal to which, and what
/// going through did to places, directions and turns, for whatever keeps its own beside the Obj's to take
/// through too.
/// <code>
/// // An orbit camera keeps its own angle round what it follows: turned with it, it stays behind it.
/// Portal.Passed += passage =>
/// {
///     if (passage.Obj == target)
///     {
///         turn += passage.Yaw;
///         followedHeight = passage.Carry(new Vector3(0f, followedHeight, 0f)).Y;
///     }
/// };
/// </code>
/// </summary>
public readonly struct PortalPassage
{
	private readonly Vector3 from;
	private readonly Vector3 to;

	internal PortalPassage(Obj obj, Portal from, Portal to, Quaternion turn, Vector3 fromPlace, Vector3 toPlace, bool atBack)
	{
		Obj = obj;
		From = from;
		To = to;
		Turn = turn;
		this.from = fromPlace;
		this.to = toPlace;
		WentInAtBack = atBack;
	}

	/// <summary>The Obj that went through.</summary>
	public Obj Obj { get; }

	/// <summary>The Portal it went in at.</summary>
	public Portal From { get; }

	/// <summary>The Portal it came out of.</summary>
	public Portal To { get; }

	/// <summary>The turn from directions on the near side to directions on the far side.</summary>
	public Quaternion Turn { get; }

	/// <summary>How far going through turned it about straight up, in degrees, as an Obj's Y rotation counts.</summary>
	public float Yaw
	{
		get
		{
			var ahead = Vector3.Transform(Vector3.UnitZ, Turn);
			return MathF.Atan2(ahead.X, ahead.Z) * (180f / MathF.PI);
		}
	}

	/// <summary>Whether it went in at the back of the Portal, and so came out of the back of the other.</summary>
	public bool WentInAtBack { get; }

	/// <summary>A place on the near side, where going through puts it on the far side.</summary>
	public Vector3 Carry(Vector3 place)
	{
		return to + Vector3.Transform(place - from, Turn);
	}

	/// <summary>A direction or a speed on the near side, as it is on the far side.</summary>
	public Vector3 CarryDirection(Vector3 direction)
	{
		return Vector3.Transform(direction, Turn);
	}

	/// <summary>A rotation on the near side, as it is on the far side.</summary>
	public Quaternion CarryRotation(Quaternion rotation)
	{
		return Quaternion.Normalize(Turn * rotation);
	}
}
