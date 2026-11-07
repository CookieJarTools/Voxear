using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's First-Person View: a camera in a character's own head. The mouse turns it, and the Obj with it; the upper
/// body leans into where it looks, and the camera rides the head, steadied, with the head hidden. It finds the body,
/// the first Model Renderer under its Obj, and the camera, the first under it or else in the level, unless told.
/// <code>
/// var view = player.AddFirstPersonView();
/// view.Sensitivity = 0.1f;
///
/// // Where it looks, for aiming:
/// var aim = view.Rotation;
/// </code>
/// </summary>
public readonly struct FirstPersonView : IEquatable<FirstPersonView>, IObjComponent
{
	private const int YawValue = 3;
	private const int PitchValue = 4;
	private const int MouseLookValue = 5;
	private const int SensitivityValue = 6;
	private const int PitchLimitsValue = 7;
	private const int BobValue = 8;
	private const int EyesValue = 9;
	private const int TurnObjValue = 10;
	private const int LeanValue = 11;
	private const int HideHeadValue = 12;
	private const int BodyValue = 13;
	private const int CameraValue = 14;

	public static readonly FirstPersonView None = default;

	internal FirstPersonView(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj it is on, which turns with it.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no First-Person View, or is gone.</summary>
	public bool IsValid => Obj.IsValid && Number(NativeComponent.Has) != 0f;

	/// <summary>Where it looks round, in degrees from the level's forward.</summary>
	public float Yaw
	{
		get { return Number(YawValue); }
		set { Set(YawValue, value); }
	}

	/// <summary>Where it looks up and down, in degrees: up is positive.</summary>
	public float Pitch
	{
		get { return Number(PitchValue); }
		set { Set(PitchValue, value); }
	}

	/// <summary>Where it looks, as a turn in the world.</summary>
	public Quaternion Rotation =>
		Quaternion.CreateFromAxisAngle(Vector3.UnitY, Mathf.Deg2Rad * Yaw) * Quaternion.CreateFromAxisAngle(Vector3.UnitX, -Mathf.Deg2Rad * Pitch);

	/// <summary>The way it looks, in the world.</summary>
	public Vector3 Forward => Vector3.Transform(Vector3.UnitZ, Rotation);

	/// <summary>Whether the mouse turns it. Off, a script turns it by Yaw and Pitch.</summary>
	public bool MouseLook
	{
		get { return Number(MouseLookValue) != 0f; }
		set { Set(MouseLookValue, value ? 1f : 0f); }
	}

	/// <summary>How far it turns for the mouse's moves, in degrees a pixel.</summary>
	public float Sensitivity
	{
		get { return Number(SensitivityValue); }
		set { Set(SensitivityValue, value); }
	}

	/// <summary>How far it may look down and up, in degrees: down as a negative.</summary>
	public Vector2 PitchLimits
	{
		get { return Get(PitchLimitsValue).AsVector2(); }
		set { NativeComponent.Set(NativeComponent.FirstPersonView, Obj, PitchLimitsValue, new Vector4(value, 0f, 0f)); }
	}

	/// <summary>How much of the head's bob reaches the camera: 0 none, 1 all of it.</summary>
	public float HeadBob
	{
		get { return Number(BobValue); }
		set { Set(BobValue, value); }
	}

	/// <summary>Where the eyes are from the head, in metres in the body's space.</summary>
	public Vector3 Eyes
	{
		get { return Get(EyesValue).AsVector3(); }
		set { NativeComponent.Set(NativeComponent.FirstPersonView, Obj, EyesValue, new Vector4(value, 0f)); }
	}

	/// <summary>Whether its Obj turns to face where it looks.</summary>
	public bool TurnObj
	{
		get { return Number(TurnObjValue) != 0f; }
		set { Set(TurnObjValue, value ? 1f : 0f); }
	}

	/// <summary>How much of its tilt the upper body leans into, from 0 to 1.</summary>
	public float Lean
	{
		get { return Number(LeanValue); }
		set { Set(LeanValue, value); }
	}

	/// <summary>Whether the head is hidden, so the camera does not see inside it.</summary>
	public bool HideHead
	{
		get { return Number(HideHeadValue) != 0f; }
		set { Set(HideHeadValue, value ? 1f : 0f); }
	}

	/// <summary>The Obj whose model is the body, or None for the first under its Obj.</summary>
	public Obj Body
	{
		get { return NativeComponent.ObjOf(Get(BodyValue)); }
		set { NativeComponent.Set(NativeComponent.FirstPersonView, Obj, BodyValue, NativeComponent.ValueOf(value)); }
	}

	/// <summary>The camera it places, or None for the first under its Obj, or else in the level.</summary>
	public Camera Camera
	{
		get { return NativeComponent.ObjOf(Get(CameraValue)).Camera; }
		set { NativeComponent.Set(NativeComponent.FirstPersonView, Obj, CameraValue, NativeComponent.ValueOf(value.Obj)); }
	}

	/// <summary>Takes the First-Person View off its Obj.</summary>
	public void Remove()
	{
		NativeComponent.Set(NativeComponent.FirstPersonView, Obj, NativeComponent.Remove, Vector4.Zero);
	}

	public static bool operator ==(FirstPersonView left, FirstPersonView right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(FirstPersonView left, FirstPersonView right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(FirstPersonView other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is FirstPersonView view && Equals(view);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s First-Person View";
	}

	private Vector4 Get(int what)
	{
		NativeComponent.Get(NativeComponent.FirstPersonView, Obj, what, out var value);
		return value;
	}

	private float Number(int what)
	{
		return NativeComponent.Number(NativeComponent.FirstPersonView, Obj, what, 0f);
	}

	private void Set(int what, float value)
	{
		NativeComponent.Set(NativeComponent.FirstPersonView, Obj, what, new Vector4(value, 0f, 0f, 0f));
	}
}
