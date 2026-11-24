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
/// // A room seen from above, 6 m up and down, every ray parallel.
/// camera.MakeOrthographic(6f);
/// </code>
/// </summary>
public readonly struct Camera : IEquatable<Camera>, IObjComponent
{
	private const int FieldOfViewValue = 3;
	private const int NearValue = 4;
	private const int ExposureValue = 5;
	private const int FollowsValue = 6;
	private const int ProjectionValue = 7;
	private const int SizeValue = 8;
	private const int FieldOfViewAxisValue = 9;
	private const int PhysicalValue = 10;
	private const int FocalLengthValue = 11;
	private const int SensorValue = 12;
	private const int GateFitValue = 13;
	private const int LensShiftValue = 14;
	private const int FarValue = 15;
	private const int ViewportValue = 16;
	private const int PriorityValue = 17;
	private const int LayersValue = 18;
	private const int TargetValue = 19;
	private const int BackgroundValue = 20;
	private const int PostValue = 21;
	private const int AntiAliasingValue = 22;
	private const int HdrValue = 23;
	private const int OcclusionCullingValue = 24;
	private const int OverValue = 25;
	private const int ClipPlaneValue = 26;
	private const int ProjectionColumnValue = 27;
	private const int NoCustomProjectionValue = 31;
	private const int ScreenValue = 32;
	private const int ScreenBrightnessValue = 33;

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

	/// <summary>Perspective, further off smaller, or orthographic, every ray parallel, as a plan or a room drawn from above.</summary>
	public CameraProjection Projection
	{
		get { return Number(ProjectionValue) >= 0.5f ? CameraProjection.Orthographic : CameraProjection.Perspective; }
		set { Set(ProjectionValue, value == CameraProjection.Orthographic ? 1f : 0f); }
	}

	/// <summary>With an orthographic projection, half the height it sees, in metres.</summary>
	public float Size
	{
		get { return Number(SizeValue); }
		set { Set(SizeValue, value); }
	}

	/// <summary>Makes it orthographic, seeing <paramref name="halfHeight"/> metres up and down from its middle.</summary>
	public void MakeOrthographic(float halfHeight)
	{
		Projection = CameraProjection.Orthographic;
		Size = halfHeight;
	}

	/// <summary>Which way FieldOfView is measured: up and down, or across.</summary>
	public FieldOfViewAxis FieldOfViewAxis
	{
		get { return Number(FieldOfViewAxisValue) >= 0.5f ? FieldOfViewAxis.Horizontal : FieldOfViewAxis.Vertical; }
		set { Set(FieldOfViewAxisValue, value == FieldOfViewAxis.Horizontal ? 1f : 0f); }
	}

	/// <summary>Whether it is a physical camera, whose FocalLength and SensorSize make its field of view.</summary>
	public bool IsPhysical
	{
		get { return Number(PhysicalValue) >= 0.5f; }
		set { Set(PhysicalValue, value ? 1f : 0f); }
	}

	/// <summary>A physical camera's lens, in millimetres: longer sees less. Setting it makes the camera physical.</summary>
	public float FocalLength
	{
		get { return Number(FocalLengthValue); }
		set { Set(FocalLengthValue, value); }
	}

	/// <summary>A physical camera's sensor, width and height in millimetres: 36 by 24 is full frame.</summary>
	public Vector2 SensorSize
	{
		get { var v = Value(SensorValue); return new Vector2(v.X, v.Y); }
		set { SetValue(SensorValue, new Vector4(value, 0f, 0f)); }
	}

	/// <summary>How a physical camera's sensor is fitted to a picture of another shape.</summary>
	public GateFit GateFit
	{
		get { return (GateFit)(int)MathF.Round(Number(GateFitValue)); }
		set { Set(GateFitValue, (float)value); }
	}

	/// <summary>A physical camera's lens moved off the middle, as a share of the picture across and up.</summary>
	public Vector2 LensShift
	{
		get { var v = Value(LensShiftValue); return new Vector2(v.X, v.Y); }
		set { SetValue(LensShiftValue, new Vector4(value, 0f, 0f)); }
	}

	/// <summary>How far it sees, in metres; 0 for as far as there is anything.</summary>
	public float Far
	{
		get { return Number(FarValue); }
		set { Set(FarValue, value); }
	}

	/// <summary>Where on the screen it draws: left, bottom, width and height as shares of the screen from 0 to 1.</summary>
	public Vector4 Viewport
	{
		get { return Value(ViewportValue); }
		set { SetValue(ViewportValue, value); }
	}

	/// <summary>Which camera draws first: the lowest first, the highest on top.</summary>
	public int Priority
	{
		get { return (int)MathF.Round(Number(PriorityValue)); }
		set { Set(PriorityValue, value); }
	}

	/// <summary>The layers whose Objs it sees.</summary>
	public LayerMask Layers
	{
		get { return new LayerMask(BitConverter.SingleToInt32Bits(Number(LayersValue))); }
		set { Set(LayersValue, BitConverter.Int32BitsToSingle(value.Value)); }
	}

	/// <summary>The display it draws to, from 0, the game's own window. Setting it stops it drawing into a picture.</summary>
	public int TargetDisplay
	{
		get { var v = Value(TargetValue); return v.X >= 0.5f ? -1 : (int)v.Y; }
		set { SetValue(TargetValue, new Vector4(0f, value, 0f, 0f)); }
	}

	/// <summary>
	/// The size of the picture it draws into in place of a display, in pixels; zero while it draws to a display. A Mesh
	/// Renderer shows it with Obj.ShowCameraPicture.
	/// </summary>
	public Vector2 PictureSize
	{
		get { var v = Value(TargetValue); return v.X >= 0.5f ? new Vector2(v.Y, v.Z) : Vector2.Zero; }
		set { SetValue(TargetValue, value == Vector2.Zero ? Vector4.Zero : new Vector4(1f, value.X, value.Y, 0f)); }
	}

	/// <summary>What fills where it sees nothing.</summary>
	public CameraBackground Background
	{
		get { return (CameraBackground)(int)MathF.Round(Value(BackgroundValue).X); }
		set { var v = Value(BackgroundValue); SetValue(BackgroundValue, new Vector4((float)value, v.Y, v.Z, v.W)); }
	}

	/// <summary>The colour of a Background of one colour; setting it makes the background that colour.</summary>
	public Color BackgroundColor
	{
		get { var v = Value(BackgroundValue); return new Color(v.Y, v.Z, v.W); }
		set { SetValue(BackgroundValue, new Vector4(1f, value.R, value.G, value.B)); }
	}

	/// <summary>Whether the level's post-processing finishes its picture; off, it is shown as lit.</summary>
	public bool PostProcessing
	{
		get { return Number(PostValue) >= 0.5f; }
		set { Set(PostValue, value ? 1f : 0f); }
	}

	/// <summary>Its own anti-aliasing, in place of the post-processing's; null for theirs.</summary>
	public AntiAliasingMode? AntiAliasing
	{
		get { var at = Number(AntiAliasingValue); return at < 0f ? null : (AntiAliasingMode)(int)at; }
		set { Set(AntiAliasingValue, value is { } mode ? (float)mode : -1f); }
	}

	/// <summary>Whether light brighter than white is kept, for bloom and the tonemap; off, it is cut at white.</summary>
	public bool Hdr
	{
		get { return Number(HdrValue) >= 0.5f; }
		set { Set(HdrValue, value ? 1f : 0f); }
	}

	/// <summary>Whether what is hidden behind nearer things is left out unseen.</summary>
	public bool OcclusionCulling
	{
		get { return Number(OcclusionCullingValue) >= 0.5f; }
		set { Set(OcclusionCullingValue, value ? 1f : 0f); }
	}

	/// <summary>
	/// The camera it is drawn over, after it and in its place, a first-person view's hands over the world: what it draws
	/// covers that camera's picture, and where it draws nothing that shows. None for a camera of its own.
	/// </summary>
	public Camera DrawnOver
	{
		get { return NativeComponent.Get(NativeComponent.Camera, Obj, OverValue, out var value) ? NativeComponent.ObjOf(value).Camera : None; }
		set { NativeComponent.Set(NativeComponent.Camera, Obj, OverValue, NativeComponent.ValueOf(value.Obj)); }
	}

	/// <summary>Stacks it over <paramref name="under"/>, drawn after it in its place with nothing behind: the easy way in.</summary>
	public void DrawOver(Camera under)
	{
		DrawnOver = under;
		Background = CameraBackground.Nothing;
		Priority = under.Priority + 1;
	}

	/// <summary>Draws it into a picture of <paramref name="width"/> by <paramref name="height"/> and shows it on <paramref name="screen"/>'s mesh.</summary>
	public void ShowOn(Obj screen, int width = 512, int height = 512)
	{
		PictureSize = new Vector2(width, height);
		screen.ShowCameraPicture(this);
	}

	/// <summary>A plane, x, y and z its way out and w its place, nearer than which nothing is drawn; zero for none.</summary>
	public Vector4 ClipPlane
	{
		get { return Value(ClipPlaneValue); }
		set { SetValue(ClipPlaneValue, value); }
	}

	/// <summary>
	/// A projection of its own in place of the one its settings make, laid out for row vectors as Matrix4x4 is; null for
	/// none. Reverse depth: 1 at the near plane.
	/// </summary>
	public Matrix4x4? CustomProjection
	{
		get
		{
			if (Number(NoCustomProjectionValue) >= 0.5f)
			{
				return null;
			}
			var (a, b, c, d) = (Value(ProjectionColumnValue), Value(ProjectionColumnValue + 1), Value(ProjectionColumnValue + 2), Value(ProjectionColumnValue + 3));
			return new Matrix4x4(a.X, a.Y, a.Z, a.W, b.X, b.Y, b.Z, b.W, c.X, c.Y, c.Z, c.W, d.X, d.Y, d.Z, d.W);
		}
		set
		{
			if (value is not { } m)
			{
				SetValue(NoCustomProjectionValue, Vector4.Zero);
				return;
			}
			SetValue(ProjectionColumnValue, new Vector4(m.M11, m.M12, m.M13, m.M14));
			SetValue(ProjectionColumnValue + 1, new Vector4(m.M21, m.M22, m.M23, m.M24));
			SetValue(ProjectionColumnValue + 2, new Vector4(m.M31, m.M32, m.M33, m.M34));
			SetValue(ProjectionColumnValue + 3, new Vector4(m.M41, m.M42, m.M43, m.M44));
		}
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

	/// <summary>
	/// The Obj it films. While a Portal's opening lies between the two, the camera's view is carried through it, so
	/// a camera that follows something through a Portal goes on seeing it through the opening until the camera
	/// crosses too. A camera under a Character Controller or a Rigidbody films that without being told.
	/// </summary>
	public Obj Follows
	{
		get { return NativeComponent.Get(NativeComponent.Camera, Obj, FollowsValue, out var value) ? NativeComponent.ObjOf(value) : Obj.None; }
		set { NativeComponent.Set(NativeComponent.Camera, Obj, FollowsValue, NativeComponent.ValueOf(value)); }
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

	private Vector4 Value(int what)
	{
		return NativeComponent.Get(NativeComponent.Camera, Obj, what, out var value) ? value : Vector4.Zero;
	}

	private void SetValue(int what, Vector4 value)
	{
		NativeComponent.Set(NativeComponent.Camera, Obj, what, value);
	}
}
