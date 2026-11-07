using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Camera: what the game is seen through. Get it with Obj.Camera, or give an Obj one with Obj.AddCamera.
/// <code>
/// // Aiming narrows the view.
/// var camera = eye.Camera;
/// camera.FieldOfView = Mathf.Lerp(70f, 55f, aiming);
/// </code>
/// </summary>
public readonly struct Camera : IEquatable<Camera>, IObjComponent
{
	private const int FieldOfViewValue = 3;
	private const int NearValue = 4;
	private const int ExposureValue = 5;

	public static readonly Camera None = default;

	internal Camera(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj it sees from.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Camera, or is gone.</summary>
	public bool IsValid => Obj.IsValid && Number(NativeComponent.Has) != 0f;

	/// <summary>How much it sees, up and down, in degrees.</summary>
	public float FieldOfView
	{
		get { return Number(FieldOfViewValue); }
		set { Set(FieldOfViewValue, value); }
	}

	/// <summary>How close it sees, in metres: nearer than this is cut away.</summary>
	public float Near
	{
		get { return Number(NearValue); }
		set { Set(NearValue, value); }
	}

	/// <summary>How bright it shows the world: 1 as lit, 2 twice as bright.</summary>
	public float Exposure
	{
		get { return Number(ExposureValue); }
		set { Set(ExposureValue, value); }
	}

	/// <summary>The way it looks, in the world.</summary>
	public Vector3 Forward => Vector3.Transform(Vector3.UnitZ, Obj.WorldRotation);

	/// <summary>Takes the Camera off its Obj.</summary>
	public void Remove()
	{
		NativeComponent.Set(NativeComponent.Camera, Obj, NativeComponent.Remove, Vector4.Zero);
	}

	public static bool operator ==(Camera left, Camera right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(Camera left, Camera right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(Camera other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is Camera camera && Equals(camera);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Camera";
	}

	private float Number(int what)
	{
		return NativeComponent.Number(NativeComponent.Camera, Obj, what, 0f);
	}

	private void Set(int what, float value)
	{
		NativeComponent.Set(NativeComponent.Camera, Obj, what, new Vector4(value, 0f, 0f, 0f));
	}
}
