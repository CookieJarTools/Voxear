using System;
using System.Numerics;
using System.Text;

namespace Voxear.Interop;

/// <summary>
/// The engine's Particle Systems, for ParticleSystem: what each value's number is, as the engine's particles.rs has
/// them. Without the engine, as in tests, no Obj has one, and every value reads as missing.
/// </summary>
internal static unsafe class NativeParticles
{
	public const int Has = 0;
	public const int Add = 1;
	public const int Remove = 2;
	public const int Play = 3;
	public const int Stop = 4;
	public const int Playing = 5;
	public const int Duration = 6;
	public const int Looping = 7;
	public const int Rate = 8;
	public const int Burst = 9;
	public const int Lifetime = 10;
	public const int Speed = 11;
	public const int Size = 12;
	public const int SizeEnd = 13;
	public const int Color = 14;
	public const int ColorEnd = 15;
	public const int Intensity = 16;
	public const int Shape = 17;
	public const int Radius = 18;
	public const int Angle = 19;
	public const int BoxSize = 20;
	public const int Gravity = 21;
	public const int Drag = 22;
	public const int Spin = 23;
	public const int Blend = 24;
	public const int Lit = 25;
	public const int Softness = 26;
	public const int Seed = 27;
	public const int Space = 28;
	public const int Wind = 29;
	public const int Turbulence = 30;
	public const int TurbulenceSize = 31;
	public const int Simulated = 32;
	public const int Collision = 33;
	public const int Bounce = 34;
	public const int Friction = 35;
	public const int LifeLost = 36;
	public const int Forces = 37;
	public const int Stretch = 38;
	public const int Delay = 39;
	public const int Light = 40;
	public const int LightRange = 41;
	public const int Trails = 42;
	public const int TrailTime = 43;
	public const int TrailWidth = 44;
	public const int HitEvents = 45;

	/// <summary>What is asked in words, through NativeComponent's texts: the effect a system plays, read and set.</summary>
	public const int Effect = 0;
	public const int SetEffect = 1;

	/// <summary>
	/// A curve or the gradient, as words: SizeCurve and the rest read it, plus Change to set it. Keys a line each,
	/// where then what; a gradient's colour as it looks on screen.
	/// </summary>
	public const int SizeCurve = 10;
	public const int OpacityCurve = 11;
	public const int SpeedCurve = 12;
	public const int ColorGradient = 13;
	public const int Change = 10;
	private const int PlayEffect = 2;
	private const int AnswerRoom = 32;

	/// <summary>A value of an Obj's Particle System, or false when the Obj has none.</summary>
	public static bool Get(Obj obj, int what, out Vector4 value)
	{
		var read = default(Vector4);
		var found = Native.IsConnected && Native.Api->ParticleValue(obj.Id, what, 0, (float*)&read) == 1;
		value = read;
		return found;
	}

	/// <summary>Writes a value of an Obj's Particle System: false when the Obj has none.</summary>
	public static bool Set(Obj obj, int what, Vector4 value)
	{
		return Native.IsConnected && Native.Api->ParticleValue(obj.Id, what, 1, (float*)&value) == 1;
	}

	public static float Number(Obj obj, int what, float missing)
	{
		return Get(obj, what, out var value) ? value.X : missing;
	}

	/// <summary>
	/// Plays an effect at a place, under an Obj or under none, as the engine's particles.rs reads it: the effect's
	/// key, where, how it is turned and its seed or nothing, a line each. The new Obj, or None. Asked once, with
	/// room for the answer, as asking twice would play it twice.
	/// </summary>
	public static Obj PlayAt(Obj under, string asked)
	{
		if (!Native.IsConnected)
		{
			return Obj.None;
		}

		var text = Encoding.UTF8.GetBytes(asked);
		var answer = stackalloc byte[AnswerRoom];
		fixed (byte* written = text)
		{
			var length = Native.Api->KindText(NativeComponent.Particles, under.Id, PlayEffect, written, text.Length, answer, AnswerRoom);
			var made = length is > 0 and <= AnswerRoom && ulong.TryParse(new ReadOnlySpan<byte>(answer, length), out var id) ? id : 0;
			return made == 0 ? Obj.None : new Obj(made);
		}
	}
}
