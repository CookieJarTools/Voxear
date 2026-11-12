using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Particle System: sparks, smoke, spray or dust sent out from the Obj. Get it with Obj.Particles, or give
/// an Obj one with Obj.AddParticles. It starts playing as it appears; Stop sends out no more, and Play starts it
/// over. Ranges are least and most, each particle somewhere between.
/// <code>
/// // A burst of sparks where something lands.
/// var sparks = hit.Obj.AddParticles();
/// sparks.Looping = false;
/// sparks.Rate = 0f;
/// sparks.Burst = 40;
/// sparks.Blend = ParticleBlend.Additive;
/// sparks.Play();
/// </code>
/// </summary>
public readonly struct ParticleSystem : IEquatable<ParticleSystem>, IObjComponent
{
	public static readonly ParticleSystem None = default;

	internal ParticleSystem(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj it sends particles out from.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Particle System, or is gone.</summary>
	public bool IsValid => Obj.IsValid && NativeParticles.Number(Obj, NativeParticles.Has, 0f) != 0f;

	/// <summary>Whether it is sending out particles: false once stopped, or once a round that does not loop is over.</summary>
	public bool IsPlaying => NativeParticles.Number(Obj, NativeParticles.Playing, 0f) != 0f;

	/// <summary>
	/// Whether its particles are kept on the graphics card and stepped each frame, as World Space, Wind and
	/// Turbulence need, rather than worked out from their age alone.
	/// </summary>
	public bool IsSimulated => NativeParticles.Number(Obj, NativeParticles.Simulated, 0f) != 0f;

	/// <summary>How long one round lasts, in seconds; it starts over after it when it loops.</summary>
	public float Duration
	{
		get { return Number(NativeParticles.Duration); }
		set { Set(NativeParticles.Duration, value); }
	}

	public bool Looping
	{
		get { return Number(NativeParticles.Looping) != 0f; }
		set { Set(NativeParticles.Looping, value ? 1f : 0f); }
	}

	/// <summary>How many particles it sends out each second.</summary>
	public float Rate
	{
		get { return Number(NativeParticles.Rate); }
		set { Set(NativeParticles.Rate, value); }
	}

	/// <summary>How many it sends out at once as each round starts.</summary>
	public int Burst
	{
		get { return (int)Number(NativeParticles.Burst); }
		set { Set(NativeParticles.Burst, value); }
	}

	/// <summary>How long each lives, in seconds.</summary>
	public Vector2 Lifetime
	{
		get { return Pair(NativeParticles.Lifetime); }
		set { Set(NativeParticles.Lifetime, new Vector4(value, 0f, 0f)); }
	}

	/// <summary>How fast each sets out, in metres a second.</summary>
	public Vector2 Speed
	{
		get { return Pair(NativeParticles.Speed); }
		set { Set(NativeParticles.Speed, new Vector4(value, 0f, 0f)); }
	}

	/// <summary>How wide each is as it starts, in metres.</summary>
	public Vector2 Size
	{
		get { return Pair(NativeParticles.Size); }
		set { Set(NativeParticles.Size, new Vector4(value, 0f, 0f)); }
	}

	/// <summary>How wide each is as it dies, against how wide it started.</summary>
	public float EndSize
	{
		get { return Number(NativeParticles.SizeEnd); }
		set { Set(NativeParticles.SizeEnd, value); }
	}

	/// <summary>Each particle's colour and opacity as it starts.</summary>
	public Color Color
	{
		get { return Colour(NativeParticles.Color); }
		set { Set(NativeParticles.Color, new Vector4(value.R, value.G, value.B, value.A)); }
	}

	/// <summary>Each particle's colour and opacity as it dies, fading from Color as it ages.</summary>
	public Color EndColor
	{
		get { return Colour(NativeParticles.ColorEnd); }
		set { Set(NativeParticles.ColorEnd, new Vector4(value.R, value.G, value.B, value.A)); }
	}

	/// <summary>How much brighter than their colour they glow, for sparks and fire.</summary>
	public float Intensity
	{
		get { return Number(NativeParticles.Intensity); }
		set { Set(NativeParticles.Intensity, value); }
	}

	public EmitShape Shape
	{
		get { return (EmitShape)(int)Number(NativeParticles.Shape); }
		set { Set(NativeParticles.Shape, (int)value); }
	}

	/// <summary>A Sphere's or a Cone's radius, in metres.</summary>
	public float Radius
	{
		get { return Number(NativeParticles.Radius); }
		set { Set(NativeParticles.Radius, value); }
	}

	/// <summary>How far out from straight up a Cone sends them, in degrees.</summary>
	public float Angle
	{
		get { return Number(NativeParticles.Angle); }
		set { Set(NativeParticles.Angle, value); }
	}

	/// <summary>A Box's size, in metres.</summary>
	public Vector3 BoxSize
	{
		get { return NativeParticles.Get(Obj, NativeParticles.BoxSize, out var value) ? new Vector3(value.X, value.Y, value.Z) : Vector3.Zero; }
		set { Set(NativeParticles.BoxSize, new Vector4(value, 0f)); }
	}

	/// <summary>How hard gravity pulls them down: 1 as hard as anything, less than 0 up, as smoke rises.</summary>
	public float Gravity
	{
		get { return Number(NativeParticles.Gravity); }
		set { Set(NativeParticles.Gravity, value); }
	}

	/// <summary>How quickly the air slows them, each second.</summary>
	public float Drag
	{
		get { return Number(NativeParticles.Drag); }
		set { Set(NativeParticles.Drag, value); }
	}

	/// <summary>How fast each turns, either way, in degrees a second.</summary>
	public float Spin
	{
		get { return Number(NativeParticles.Spin); }
		set { Set(NativeParticles.Spin, value); }
	}

	public ParticleBlend Blend
	{
		get { return (ParticleBlend)(int)Number(NativeParticles.Blend); }
		set { Set(NativeParticles.Blend, (int)value); }
	}

	/// <summary>Lit by the sun, the sky and lamps, as smoke is; or showing their colour as it is, as sparks do.</summary>
	public bool Lit
	{
		get { return Number(NativeParticles.Lit) != 0f; }
		set { Set(NativeParticles.Lit, value ? 1f : 0f); }
	}

	/// <summary>How far, in metres, they fade out before what they meet.</summary>
	public float Softness
	{
		get { return Number(NativeParticles.Softness); }
		set { Set(NativeParticles.Softness, value); }
	}

	/// <summary>What its particles move in: with their Obj, or left behind in the world as it moves on.</summary>
	public ParticleSpace Space
	{
		get { return (ParticleSpace)(int)Number(NativeParticles.Space); }
		set { Set(NativeParticles.Space, (int)value); }
	}

	/// <summary>How much of the wind they pick up: 0 none of it, 1 carried along at its speed, as smoke is.</summary>
	public float Wind
	{
		get { return Number(NativeParticles.Wind); }
		set { Set(NativeParticles.Wind, value); }
	}

	/// <summary>How fast swirls in the air stir them about, in metres a second.</summary>
	public float Turbulence
	{
		get { return Number(NativeParticles.Turbulence); }
		set { Set(NativeParticles.Turbulence, value); }
	}

	/// <summary>How wide those swirls are, in metres.</summary>
	public float TurbulenceSize
	{
		get { return Number(NativeParticles.TurbulenceSize); }
		set { Set(NativeParticles.TurbulenceSize, value); }
	}

	/// <summary>What its particles meet on their way, and bounce off or stop at.</summary>
	public ParticleCollision Collision
	{
		get { return (ParticleCollision)(int)Number(NativeParticles.Collision); }
		set { Set(NativeParticles.Collision, (int)value); }
	}

	/// <summary>How much of its speed into what it meets a particle keeps, away from it: 0 stops dead, 1 bounces as fast as it came.</summary>
	public float Bounce
	{
		get { return Number(NativeParticles.Bounce); }
		set { Set(NativeParticles.Bounce, value); }
	}

	/// <summary>How much of its speed along what it meets a particle loses each time: 0 slides on, 1 stops.</summary>
	public float Friction
	{
		get { return Number(NativeParticles.Friction); }
		set { Set(NativeParticles.Friction, value); }
	}

	/// <summary>How much of the life it has left a particle loses each time it strikes something: 1 and it dies there.</summary>
	public float LifeLost
	{
		get { return Number(NativeParticles.LifeLost); }
		set { Set(NativeParticles.LifeLost, value); }
	}

	/// <summary>How much Force Fields, blasts and what passes by move its particles: 0 not at all.</summary>
	public float Forces
	{
		get { return Number(NativeParticles.Forces); }
		set { Set(NativeParticles.Forces, value); }
	}

	/// <summary>How much longer each particle is drawn along the way it is going, in metres for each metre a second: sparks and rain as streaks.</summary>
	public float Stretch
	{
		get { return Number(NativeParticles.Stretch); }
		set { Set(NativeParticles.Stretch, value); }
	}

	/// <summary>How long after it starts before it sends anything out, in seconds.</summary>
	public float Delay
	{
		get { return Number(NativeParticles.Delay); }
		set { Set(NativeParticles.Delay, value); }
	}

	/// <summary>How bright a light it casts on what is round it, as a Point Light's Intensity: 0 none. A stream's flickers, and a burst's flashes and fades.</summary>
	public float Light
	{
		get { return Number(NativeParticles.Light); }
		set { Set(NativeParticles.Light, value); }
	}

	/// <summary>How far its light reaches, in metres.</summary>
	public float LightRange
	{
		get { return Number(NativeParticles.LightRange); }
		set { Set(NativeParticles.LightRange, value); }
	}

	/// <summary>What its particles leave behind them: a tail each, or one ribbon through them all. Either makes it simulated.</summary>
	public ParticleTrails Trails
	{
		get { return (ParticleTrails)(int)Number(NativeParticles.Trails); }
		set { Set(NativeParticles.Trails, (int)value); }
	}

	/// <summary>How long a particle's tail is, in seconds of the way it has come.</summary>
	public float TrailTime
	{
		get { return Number(NativeParticles.TrailTime); }
		set { Set(NativeParticles.TrailTime, value); }
	}

	/// <summary>How wide a tail or the ribbon is, against the particles.</summary>
	public float TrailWidth
	{
		get { return Number(NativeParticles.TrailWidth); }
		set { Set(NativeParticles.TrailWidth, value); }
	}

	/// <summary>Which particles: two systems alike but for this send out different ones.</summary>
	public int Seed
	{
		get { return (int)Number(NativeParticles.Seed); }
		set { Set(NativeParticles.Seed, value); }
	}

	/// <summary>Starts it over from the start of a round, with its burst.</summary>
	public void Play()
	{
		Set(NativeParticles.Play, 0f);
	}

	/// <summary>Sends out no more particles. Those already out live on.</summary>
	public void Stop()
	{
		Set(NativeParticles.Stop, 0f);
	}

	/// <summary>Takes the Particle System off its Obj, and its particles with it.</summary>
	public void Remove()
	{
		NativeParticles.Set(Obj, NativeParticles.Remove, Vector4.Zero);
	}

	public static bool operator ==(ParticleSystem left, ParticleSystem right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(ParticleSystem left, ParticleSystem right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(ParticleSystem other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is ParticleSystem system && Equals(system);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Particle System";
	}

	private float Number(int what)
	{
		return NativeParticles.Number(Obj, what, 0f);
	}

	private Vector2 Pair(int what)
	{
		return NativeParticles.Get(Obj, what, out var value) ? new Vector2(value.X, value.Y) : Vector2.Zero;
	}

	private Color Colour(int what)
	{
		return NativeParticles.Get(Obj, what, out var value) ? new Color(value.X, value.Y, value.Z, value.W) : Color.White;
	}

	private void Set(int what, float value)
	{
		Set(what, new Vector4(value, 0f, 0f, 0f));
	}

	private void Set(int what, Vector4 value)
	{
		if (!NativeParticles.Set(Obj, what, value) && Native.IsConnected)
		{
			Log.Error($"{Obj} has no Particle System; give it one with AddParticles.");
		}
	}
}
