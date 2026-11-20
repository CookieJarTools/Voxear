using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Light: a sun, a lamp, a spot, or a glowing shape. Get it with Obj.Light, or give an Obj one with
/// Obj.AddLight. It shines along the Obj's forward (Z), so turn the Obj to aim it.
/// <code>
/// // A lamp that flickers.
/// protected override void Update()
/// {
///     var lamp = Obj.Light;
///     var time = (float)Time.Elapsed;
///     lamp.Intensity = 2f + 0.4f * MathF.Sin(time * 23f) * MathF.Sin(time * 7f);
/// }
/// </code>
/// </summary>
public readonly struct Light : IEquatable<Light>, IObjComponent
{
	public static readonly Light None = default;

	internal Light(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj that shines.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Light, or is gone.</summary>
	public bool IsValid => Obj.IsValid && NativeLights.Number(Obj, NativeLights.Has, 0f) != 0f;

	/// <summary>
	/// What shape it shines from. Changing it starts from that kind's own settings, keeping its colour, temperature,
	/// intensity and shadows.
	/// </summary>
	public LightKind Kind
	{
		get { return (LightKind)(int)NativeLights.Number(Obj, NativeLights.Kind, (float)LightKind.Point); }
		set { Set(NativeLights.Kind, new Vector4((int)value, 0f, 0f, 0f)); }
	}

	/// <summary>Its colour, as it looks on screen. With a Temperature, a filter over that temperature's colour.</summary>
	public Color Color
	{
		get
		{
			return NativeLights.Get(Obj, NativeLights.Color, out var value) ? new Color(value.X, value.Y, value.Z) : Color.White;
		}
		set { Set(NativeLights.Color, new Vector4(value.R, value.G, value.B, 1f)); }
	}

	/// <summary>How bright it is. A shape gives as much light as a point of this intensity, whatever its size.</summary>
	public float Intensity
	{
		get { return NativeLights.Number(Obj, NativeLights.Intensity, 0f); }
		set { Set(NativeLights.Intensity, value); }
	}

	/// <summary>How far it reaches, in metres, from the nearest point of its shape. A Directional light reaches everywhere.</summary>
	public float Range
	{
		get { return NativeLights.Number(Obj, NativeLights.Range, 0f); }
		set { Set(NativeLights.Range, value); }
	}

	/// <summary>A Spot's cone, in degrees across: full brightness inside the inner angle, fading out to the outer.</summary>
	public float InnerSpotAngle
	{
		get { return SpotAngles().X; }
		set { Set(NativeLights.SpotAngles, new Vector4(value, SpotAngles().Y, 0f, 0f)); }
	}

	/// <summary>Where a Spot's cone ends, in degrees across. The inner angle stays within it.</summary>
	public float OuterSpotAngle
	{
		get { return SpotAngles().Y; }
		set { Set(NativeLights.SpotAngles, new Vector4(SpotAngles().X, value, 0f, 0f)); }
	}

	/// <summary>
	/// The colour of a light this hot, in kelvin: 1900 a candle, 2700 a household bulb, 6500 daylight. Null lights in
	/// its Color alone.
	/// </summary>
	public float? Temperature
	{
		get
		{
			var kelvin = NativeLights.Number(Obj, NativeLights.Temperature, 0f);
			return kelvin > 0f ? kelvin : null;
		}
		set { Set(NativeLights.Temperature, value ?? 0f); }
	}

	/// <summary>What of it a lighting bake keeps: nothing, its bounce (the default), or its direct light too.</summary>
	public LightMode Mode
	{
		get { return (LightMode)(int)NativeLights.Number(Obj, NativeLights.Mode, 1f); }
		set { Set(NativeLights.Mode, (float)(int)value); }
	}

	/// <summary>Whether it casts shadows. A Directional light always casts its own.</summary>
	public bool Shadows
	{
		get { return NativeLights.Number(Obj, NativeLights.Shadows, 0f) != 0f; }
		set { Set(NativeLights.Shadows, value ? 1f : 0f); }
	}

	/// <summary>A Rectangle's size, in metres: across its X and up its Y.</summary>
	public Vector2 Size
	{
		get { return NativeLights.Get(Obj, NativeLights.Size, out var value) ? new Vector2(value.X, value.Y) : Vector2.Zero; }
		set { Set(NativeLights.Size, new Vector4(value.X, value.Y, 0f, 0f)); }
	}

	/// <summary>
	/// A Disc's or a Tube's radius, in metres; for a Point or Spot light, its bulb's or flame's, which softens its
	/// shadow (0 for hard edges).
	/// </summary>
	public float Radius
	{
		get { return NativeLights.Number(Obj, NativeLights.Radius, 0f); }
		set { Set(NativeLights.Radius, value); }
	}

	/// <summary>A Tube's length, end to end, in metres.</summary>
	public float Length
	{
		get { return NativeLights.Number(Obj, NativeLights.Length, 0f); }
		set { Set(NativeLights.Length, value); }
	}

	/// <summary>
	/// How wide the sun looks, in degrees, which softens its shadows' edges the farther they fall: about 0.5 for the
	/// real sun, 0 for hard edges. A Rectangle, Disc or Tube softens its shadow by its own size (see SoftShadows).
	/// </summary>
	public float Softness
	{
		get { return NativeLights.Number(Obj, NativeLights.Softness, 0f); }
		set { Set(NativeLights.Softness, value); }
	}

	/// <summary>
	/// Whether a local light's shadow softens as wide as the light is, hard where things touch and softer the farther it
	/// falls from them: a Rectangle, Disc or Tube by its shape, a Point or Spot light by its Radius. On by default; off,
	/// its edges are hard and cost less to shade.
	/// </summary>
	public bool SoftShadows
	{
		get { return NativeLights.Number(Obj, NativeLights.SoftShadows, 1f) != 0f; }
		set { Set(NativeLights.SoftShadows, value ? 1f : 0f); }
	}

	/// <summary>
	/// A picture it shines through, its colours tinting the light: projected over a Spot light's cone, or tiled over
	/// the world by a Directional light every CookieSize metres. Texture.None for none.
	/// </summary>
	public Texture Cookie
	{
		get { return NativeWhole.Asset(NativeWhole.LightCookie, Obj) is { } guid ? Texture.Load(guid) : Texture.None; }
		set { SetAsset(NativeWhole.LightCookie, value.IsSet ? value.Reference : string.Empty); }
	}

	/// <summary>How far apart a Directional light's cookie repeats over the world, in metres.</summary>
	public float CookieSize
	{
		get { return NativeLights.Number(Obj, NativeLights.CookieSize, 1f); }
		set { Set(NativeLights.CookieSize, value); }
	}

	/// <summary>An IES profile a Point or Spot light shines by: how bright a real fitting is in each direction.</summary>
	public LightProfile Profile
	{
		get { return NativeWhole.Asset(NativeWhole.LightProfile, Obj) is { } guid ? LightProfile.Load(guid) : LightProfile.None; }
		set { SetAsset(NativeWhole.LightProfile, value.IsSet ? value.Reference : string.Empty); }
	}

	/// <summary>
	/// The colour a Directional light's shadows take, as light through a canopy or off warm ground fills them; black
	/// leaves them as they are.
	/// </summary>
	public Color ShadowTint
	{
		get { return NativeLights.Get(Obj, NativeLights.ShadowTint, out var value) ? new Color(value.X, value.Y, value.Z) : Color.Black; }
		set { Set(NativeLights.ShadowTint, new Vector4(value.R, value.G, value.B, 1f)); }
	}

	/// <summary>What a Directional light is in the sky: the sun, or the moon, under which the sky is a night sky.</summary>
	public SkyBody Body
	{
		get { return NativeLights.Number(Obj, NativeLights.Body, 0f) != 0f ? SkyBody.Moon : SkyBody.Sun; }
		set { Set(NativeLights.Body, value == SkyBody.Moon ? 1f : 0f); }
	}

	/// <summary>Takes the Light off its Obj.</summary>
	public void Remove()
	{
		NativeLights.Set(Obj, NativeLights.Remove, Vector4.Zero);
	}

	public static bool operator ==(Light left, Light right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(Light left, Light right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(Light other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is Light light && Equals(light);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Light";
	}

	private Vector2 SpotAngles()
	{
		return NativeLights.Get(Obj, NativeLights.SpotAngles, out var value) ? new Vector2(value.X, value.Y) : Vector2.Zero;
	}

	private void Set(int what, float value)
	{
		Set(what, new Vector4(value, 0f, 0f, 0f));
	}

	private void Set(int what, Vector4 value)
	{
		if (!NativeLights.Set(Obj, what, value) && Native.IsConnected)
		{
			Log.Error($"{Obj} has no Light; give it one with AddLight.");
		}
	}

	private void SetAsset(int which, string reference)
	{
		if (!NativeWhole.SetAsset(which, Obj, reference) && Native.IsConnected && !IsValid)
		{
			Log.Error($"{Obj} has no Light; give it one with AddLight.");
		}
	}
}
