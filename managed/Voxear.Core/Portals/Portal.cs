using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Portal: an opening that shows the place another Portal stands in, and takes whatever goes through it
/// there. The opening is as wide as Size.X about the Obj and as high as Size.Y up from it, and faces along the
/// Obj's forward. Get it with Obj.Portal, give an Obj one with Obj.AddPortal, or join two with Portal.Link.
/// <code>
/// // Two gates that lead to each other.
/// Portal.Link(townGate, campGate);
///
/// // The camp's gate opens once the camp is found.
/// campGate.Portal.IsOpen = campFound;
/// </code>
/// </summary>
public readonly struct Portal : IEquatable<Portal>, IObjComponent
{
	private const int ToValue = 3;
	private const int SizeValue = 4;
	private const int OpenValue = 5;
	private const int PassesValue = 6;
	private const int MistValue = 7;
	private const int ViewDistanceValue = 8;
	private const int ReadyValue = 9;
	private const int PassedCountValue = 20;
	private const int BetweenFromValue = 30;
	private const int BetweenToValue = 31;
	private const int BetweenValue = 32;
	private const int CreateValue = 33;
	private const int CreatedValue = 34;
	private const int PassedValue = 100;
	private const int PassedKept = 64;

	public static readonly Portal None = default;

	private static uint told;
	private static bool started;

	internal Portal(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>
	/// Raised for each Obj that goes through a Portal, once it has come out of the far one: before LateUpdate in the
	/// frame it went through, so what follows it can follow it there.
	/// </summary>
	public static event Action<PortalPassage>? Passed;

	/// <summary>The Obj it belongs to.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Portal, or is gone.</summary>
	public bool IsValid => Obj.IsValid && Number(NativeComponent.Has) != 0f;

	/// <summary>The Portal it leads to. With none it shows nothing and nothing goes through it.</summary>
	public Portal To
	{
		get { return NativeComponent.Get(NativeComponent.Portal, Obj, ToValue, out var value) ? new Portal(NativeComponent.ObjOf(value)) : None; }
		set { Write(ToValue, NativeComponent.ValueOf(value.Obj)); }
	}

	/// <summary>The opening's width and height, in metres.</summary>
	public Vector2 Size
	{
		get { return NativeComponent.Get(NativeComponent.Portal, Obj, SizeValue, out var value) ? new Vector2(value.X, value.Y) : Vector2.Zero; }
		set { Write(SizeValue, new Vector4(value.X, value.Y, 0f, 0f)); }
	}

	/// <summary>Whether it shows its far side and lets Objs through. Closed, it shows its mist.</summary>
	public bool IsOpen
	{
		get { return Number(OpenValue) != 0f; }
		set { Write(OpenValue, new Vector4(value ? 1f : 0f, 0f, 0f, 0f)); }
	}

	/// <summary>
	/// Whether its far side is there to see and to stand on. In a world without end the ground round the far Portal
	/// is made as the player comes near, and the Portal shows its mist until it is.
	/// </summary>
	public bool IsReady => Number(ReadyValue) != 0f;

	/// <summary>The layers whose Objs go through it.</summary>
	public LayerMask Passes
	{
		get { return NativeComponent.Get(NativeComponent.Portal, Obj, PassesValue, out var value) ? new LayerMask(BitConverter.SingleToInt32Bits(value.X)) : LayerMask.Nothing; }
		set { Write(PassesValue, new Vector4(BitConverter.Int32BitsToSingle(value.Value), 0f, 0f, 0f)); }
	}

	/// <summary>The colour of its mist, which shows while it is closed or its far side is not made yet.</summary>
	public Color Mist
	{
		get { return NativeComponent.Get(NativeComponent.Portal, Obj, MistValue, out var value) ? new Color(value.X, value.Y, value.Z) : default; }
		set { Write(MistValue, new Vector4(value.R, value.G, value.B, 1f)); }
	}

	/// <summary>How far from its opening a camera still sees its far side, in metres.</summary>
	public float ViewDistance
	{
		get { return Number(ViewDistanceValue); }
		set { Write(ViewDistanceValue, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>
	/// A new Obj named Portal with a Portal on it, its opening <paramref name="size"/> wide and high, standing at
	/// <paramref name="position"/> (the middle of its sill) and facing along <paramref name="rotation"/>'s forward.
	/// It leads nowhere until its To is set, or it is joined to another with Link.
	/// </summary>
	public static Portal Create(Vector3 position, Quaternion rotation, Vector2 size)
	{
		NativeComponent.Set(NativeComponent.Portal, Obj.None, CreateValue, new Vector4(position, 0f));
		if (!NativeComponent.Get(NativeComponent.Portal, Obj.None, CreatedValue, out var value))
		{
			return None;
		}
		var obj = NativeComponent.ObjOf(value);
		obj.Rotation = rotation;
		var portal = new Portal(obj);
		portal.Size = size;
		return portal;
	}

	/// <summary>
	/// Two new Portals of the same size that lead to each other: one at each place, each facing along its rotation's
	/// forward. Walk into the front of either and out of the front of the other.
	/// <code>
	/// var size = new Vector2(3f, 4f);
	/// var (town, camp) = Portal.CreatePair(townSquare, Quaternion.Identity, campfire, Quaternion.Identity, size);
	/// camp.Close();
	/// </code>
	/// </summary>
	public static (Portal First, Portal Second) CreatePair(Vector3 first, Quaternion firstRotation, Vector3 second, Quaternion secondRotation, Vector2 size)
	{
		var (a, b) = (Create(first, firstRotation, size), Create(second, secondRotation, size));
		a.To = b;
		b.To = a;
		return (a, b);
	}

	/// <summary>Gives both Objs a Portal, unless they have one, each leading to the other.</summary>
	public static void Link(Obj first, Obj second)
	{
		var (a, b) = (first.AddPortal(), second.AddPortal());
		a.To = b;
		b.To = a;
	}

	/// <summary>
	/// The open Portal whose opening lies between two places, or None. A camera that keeps clear of walls asks it
	/// before it looks for what is in its way: with a Portal between it and what it follows, the line it would
	/// test runs on from the far side of the opening, where nothing is in its way.
	/// <code>
	/// var wanted = pivot + back * distance;
	/// var clear = Portal.Between(pivot, wanted).IsValid || !Physics.Linecast(pivot, wanted, out var hit);
	/// </code>
	/// </summary>
	public static Portal Between(Vector3 from, Vector3 to)
	{
		NativeComponent.Set(NativeComponent.Portal, Obj.None, BetweenFromValue, new Vector4(from, 0f));
		NativeComponent.Set(NativeComponent.Portal, Obj.None, BetweenToValue, new Vector4(to, 0f));
		return NativeComponent.Get(NativeComponent.Portal, Obj.None, BetweenValue, out var value) ? new Portal(NativeComponent.ObjOf(value)) : None;
	}

	/// <summary>Lets Objs through and shows its far side.</summary>
	public void Open()
	{
		IsOpen = true;
	}

	/// <summary>Shows its mist and lets nothing through.</summary>
	public void Close()
	{
		IsOpen = false;
	}

	/// <summary>Takes the Portal off its Obj.</summary>
	public void Remove()
	{
		Write(NativeComponent.Remove, Vector4.Zero);
	}

	/// <summary>Raises Passed for each Obj that went through a Portal since it was last called.</summary>
	internal static void Dispatch()
	{
		if (!NativeComponent.Get(NativeComponent.Portal, Obj.None, PassedCountValue, out var value))
		{
			return;
		}
		var count = BitConverter.SingleToUInt32Bits(value.X);
		if (!started)
		{
			// Those from before scripts started are not theirs to hear.
			(started, told) = (true, count);
		}
		// Further behind than are kept, the oldest are gone.
		if (count - told > PassedKept)
		{
			told = count - PassedKept;
		}
		for (; told != count; told++)
		{
			var passage = Read((int)(told % PassedKept));
			if (passage.Obj.IsValid)
			{
				Passed?.Invoke(passage);
			}
		}
	}

	private static PortalPassage Read(int slot)
	{
		var first = PassedValue + slot * 8;
		Vector4 Value(int which)
		{
			return NativeComponent.Get(NativeComponent.Portal, Obj.None, first + which, out var read) ? read : default;
		}
		var turn = Value(3);
		var (from, to) = (Value(4), Value(5));
		return new PortalPassage(
			NativeComponent.ObjOf(Value(0)),
			new Portal(NativeComponent.ObjOf(Value(1))),
			new Portal(NativeComponent.ObjOf(Value(2))),
			new Quaternion(turn.X, turn.Y, turn.Z, turn.W),
			new Vector3(from.X, from.Y, from.Z),
			new Vector3(to.X, to.Y, to.Z),
			Value(6).X != 0f);
	}

	private float Number(int what)
	{
		return NativeComponent.Number(NativeComponent.Portal, Obj, what, 0f);
	}

	private void Write(int what, Vector4 value)
	{
		NativeComponent.Set(NativeComponent.Portal, Obj, what, value);
	}

	public static bool operator ==(Portal left, Portal right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(Portal left, Portal right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(Portal other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is Portal same && Equals(same);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Portal";
	}
}
