using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Gun: what kind of gun its model is, how heavy and long, how hard it kicks, and its sockets, the places hands
/// and eyes go, in the model's own space with the barrel along +Z from the grip. A Gun Holder holds it; setting its Kind
/// fills in a typical gun's numbers.
/// <code>
/// var rifle = model.AddGun();
/// rifle.Kind = GunKind.BoltActionRifle;
/// rifle.Mass = 4.1f;
/// </code>
/// </summary>
public readonly struct Gun : IEquatable<Gun>, IObjComponent
{
	private const int KindValue = 3;
	private const int MassValue = 4;
	private const int LengthValue = 5;
	private const int FireRateValue = 6;
	private const int RecoilValue = 7;
	private const int GripValue = 8;
	private const int SupportValue = 9;
	private const int RearSightValue = 10;
	private const int FrontSightValue = 11;
	private const int EyeReliefValue = 12;
	private const int AimFovValue = 13;
	private const int MuzzleValue = 14;
	private const int RoundsValue = 15;
	private const int ChamberValue = 16;
	private const int FeedKnownValue = 17;
	private const int ChamberKnownValue = 18;
	private const int CapacityValue = 19;
	private const int OpenValue = 20;

	public static readonly Gun None = default;

	internal Gun(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The gun's model.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Gun, or is gone.</summary>
	public bool IsValid => Obj.IsValid && Number(NativeComponent.Has) != 0f;

	/// <summary>How it works. Setting it fills in a typical gun of the kind's numbers and sockets.</summary>
	public GunKind Kind
	{
		get { return (GunKind)(int)Number(KindValue); }
		set { Set(KindValue, (int)value); }
	}

	/// <summary>How heavy it is, in kilograms: heavier guns sway more and settle slower.</summary>
	public float Mass
	{
		get { return Number(MassValue); }
		set { Set(MassValue, value); }
	}

	/// <summary>How long it is, in metres.</summary>
	public float Length
	{
		get { return Number(LengthValue); }
		set { Set(LengthValue, value); }
	}

	/// <summary>Rounds a minute it can fire.</summary>
	public float FireRate
	{
		get { return Number(FireRateValue); }
		set { Set(FireRateValue, value); }
	}

	/// <summary>How hard each shot kicks, 1 for a full-power rifle round.</summary>
	public float Recoil
	{
		get { return Number(RecoilValue); }
		set { Set(RecoilValue, value); }
	}

	/// <summary>Where the shooting hand's wrist goes, on the model.</summary>
	public Vector3 Grip
	{
		get { return Point(GripValue); }
		set { Set(GripValue, value); }
	}

	/// <summary>Where the other hand's wrist goes, on the model.</summary>
	public Vector3 Support
	{
		get { return Point(SupportValue); }
		set { Set(SupportValue, value); }
	}

	/// <summary>The rear sight, on the model.</summary>
	public Vector3 RearSight
	{
		get { return Point(RearSightValue); }
		set { Set(RearSightValue, value); }
	}

	/// <summary>The front sight, on the model.</summary>
	public Vector3 FrontSight
	{
		get { return Point(FrontSightValue); }
		set { Set(FrontSightValue, value); }
	}

	/// <summary>How far the eye is from the rear sight when aiming, in metres.</summary>
	public float EyeRelief
	{
		get { return Number(EyeReliefValue); }
		set { Set(EyeReliefValue, value); }
	}

	/// <summary>The field of view when aiming, in degrees; 0 leaves it as it is.</summary>
	public float AimFov
	{
		get { return Number(AimFovValue); }
		set { Set(AimFovValue, value); }
	}

	/// <summary>The muzzle, on the model, where shots leave.</summary>
	public Vector3 Muzzle
	{
		get { return Point(MuzzleValue); }
		set { Set(MuzzleValue, value); }
	}

	/// <summary>Rounds in the feed now, the chambered one apart: the truth, whatever the holder knows of it.</summary>
	public int Rounds
	{
		get { return (int)Number(RoundsValue); }
		set { Set(RoundsValue, value); }
	}

	/// <summary>How many rounds the feed holds.</summary>
	public int Capacity => (int)Number(CapacityValue);

	/// <summary>Whether a live round is in the chamber.</summary>
	public bool Chambered => Number(ChamberValue) == 1f;

	/// <summary>Whether the action is held open: a slide locked back on empty, or an open bolt cocked.</summary>
	public bool IsOpen => Number(OpenValue) != 0f;

	/// <summary>What the holder knows of the rounds in the feed.</summary>
	public AmmoKnowledge FeedKnown => (AmmoKnowledge)(int)Number(FeedKnownValue);

	/// <summary>What the holder knows of the chamber.</summary>
	public AmmoKnowledge ChamberKnown => (AmmoKnowledge)(int)Number(ChamberKnownValue);

	/// <summary>
	/// About how full the feed is, as the holder last saw it, or null when they have not looked since firing from it.
	/// <code>
	/// ammoLabel.Text = gun.Estimate switch
	/// {
	/// 	AmmoEstimate.Full => "Full",
	/// 	AmmoEstimate.Empty => "Empty",
	/// 	null => "?",
	/// 	_ => "Some left",
	/// };
	/// </code>
	/// </summary>
	public AmmoEstimate? Estimate
	{
		get
		{
			if (FeedKnown == AmmoKnowledge.Unknown || Capacity == 0)
			{
				return null;
			}

			var share = (float)Rounds / Capacity;
			return share switch
			{
				<= 0.05f => AmmoEstimate.Empty,
				< 0.4f => AmmoEstimate.UnderHalf,
				<= 0.6f => AmmoEstimate.Half,
				< 0.95f => AmmoEstimate.OverHalf,
				_ => AmmoEstimate.Full,
			};
		}
	}

	/// <summary>Where the muzzle is in the world now.</summary>
	public Vector3 MuzzlePosition => Obj.WorldPosition + Vector3.Transform(Muzzle, Obj.WorldRotation);

	/// <summary>The way the barrel points in the world now.</summary>
	public Vector3 Forward => Vector3.Transform(Vector3.UnitZ, Obj.WorldRotation);

	/// <summary>Takes the Gun off its Obj.</summary>
	public void Remove()
	{
		NativeComponent.Set(NativeComponent.Gun, Obj, NativeComponent.Remove, Vector4.Zero);
	}

	public static bool operator ==(Gun left, Gun right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(Gun left, Gun right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(Gun other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is Gun gun && Equals(gun);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Gun";
	}

	private float Number(int what)
	{
		return NativeComponent.Number(NativeComponent.Gun, Obj, what, 0f);
	}

	private Vector3 Point(int what)
	{
		NativeComponent.Get(NativeComponent.Gun, Obj, what, out var value);
		return value.AsVector3();
	}

	private void Set(int what, float value)
	{
		NativeComponent.Set(NativeComponent.Gun, Obj, what, new Vector4(value, 0f, 0f, 0f));
	}

	private void Set(int what, Vector3 value)
	{
		NativeComponent.Set(NativeComponent.Gun, Obj, what, new Vector4(value, 0f));
	}
}
