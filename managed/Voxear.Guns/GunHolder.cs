using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Gun Holder: a character holding a gun in its own hands, before its First-Person View. The gun is held at the
/// hip or raised to the eye with Aiming, and sways as the view turns, lags the body's moves, bobs at each footfall,
/// breathes and pulls back from walls, the hands on it all the while. It holds the first Gun under its Obj, unless told.
/// <code>
/// var holder = player.AddGunHolder();
///
/// protected override void Update()
/// {
/// 	holder.Aiming = Input.Held("Aim");
/// }
/// </code>
/// </summary>
public readonly struct GunHolder : IEquatable<GunHolder>, IObjComponent
{
	private const int GunValue = 3;
	private const int AimingValue = 4;
	private const int AimedValue = 5;
	private const int HipValue = 6;
	private const int AimTimeValue = 7;
	private const int SwayValue = 8;
	private const int BobValue = 9;
	private const int BreathingValue = 10;
	private const int WallCheckValue = 11;

	public static readonly GunHolder None = default;

	internal GunHolder(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The character holding it.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Gun Holder, or is gone.</summary>
	public bool IsValid => Obj.IsValid && Number(NativeComponent.Has) != 0f;

	/// <summary>The gun held, or None for the first under its Obj.</summary>
	public Gun Gun
	{
		get
		{
			NativeComponent.Get(NativeComponent.GunHolder, Obj, GunValue, out var value);
			return NativeComponent.ObjOf(value).Gun;
		}
		set { NativeComponent.Set(NativeComponent.GunHolder, Obj, GunValue, NativeComponent.ValueOf(value.Obj)); }
	}

	/// <summary>Whether it aims down the sights, rather than holding at the hip.</summary>
	public bool Aiming
	{
		get { return Number(AimingValue) != 0f; }
		set { Set(AimingValue, value ? 1f : 0f); }
	}

	/// <summary>How far raised to the eye the gun is now, from 0 at the hip to 1 aiming.</summary>
	public float Aimed => Number(AimedValue);

	/// <summary>Where the grip is held at the hip, from the camera, in metres: right, up and forward.</summary>
	public Vector3 Hip
	{
		get
		{
			NativeComponent.Get(NativeComponent.GunHolder, Obj, HipValue, out var value);
			return value.AsVector3();
		}
		set { NativeComponent.Set(NativeComponent.GunHolder, Obj, HipValue, new Vector4(value, 0f)); }
	}

	/// <summary>How long raising the gun to the eye takes, in seconds.</summary>
	public float AimTime
	{
		get { return Number(AimTimeValue); }
		set { Set(AimTimeValue, value); }
	}

	/// <summary>How much it sways and lags: 1 as the gun's weight has it, 0 not at all.</summary>
	public float Sway
	{
		get { return Number(SwayValue); }
		set { Set(SwayValue, value); }
	}

	/// <summary>How much it bobs with the steps: 1 as the gun's weight has it, 0 not at all.</summary>
	public float Bob
	{
		get { return Number(BobValue); }
		set { Set(BobValue, value); }
	}

	/// <summary>How much it breathes and trembles: 1 as the gun's weight has it, 0 not at all.</summary>
	public float Breathing
	{
		get { return Number(BreathingValue); }
		set { Set(BreathingValue, value); }
	}

	/// <summary>Whether it pulls back from walls.</summary>
	public bool WallCheck
	{
		get { return Number(WallCheckValue) != 0f; }
		set { Set(WallCheckValue, value ? 1f : 0f); }
	}

	/// <summary>Takes the Gun Holder off its Obj.</summary>
	public void Remove()
	{
		NativeComponent.Set(NativeComponent.GunHolder, Obj, NativeComponent.Remove, Vector4.Zero);
	}

	public static bool operator ==(GunHolder left, GunHolder right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(GunHolder left, GunHolder right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(GunHolder other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is GunHolder holder && Equals(holder);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Gun Holder";
	}

	private float Number(int what)
	{
		return NativeComponent.Number(NativeComponent.GunHolder, Obj, what, 0f);
	}

	private void Set(int what, float value)
	{
		NativeComponent.Set(NativeComponent.GunHolder, Obj, what, new Vector4(value, 0f, 0f, 0f));
	}
}
