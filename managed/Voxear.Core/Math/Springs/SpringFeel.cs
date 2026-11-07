using System;

namespace Voxear;

/// <summary>
/// How a spring moves. <see cref="Frequency"/> in hertz is how fast. <see cref="Damping"/> below 1 overshoots and rings, 1
/// settles as fast as it can without overshooting, above 1 creeps in. <see cref="Response"/> is how it answers its goal
/// jumping: 0 starts from rest, 1 heads off at once, and below 0 starts the wrong way first, as anticipation.
/// <code>
/// var sway = new Spring3(Vector3.Zero, new SpringFeel(2.5f, 0.4f));
/// var settle = new Spring(0f, SpringFeel.HalfLife(0.1f));
/// </code>
/// </summary>
public readonly record struct SpringFeel(float Frequency, float Damping, float Response = 0f)
{
	/// <summary>Quick, with a little overshoot.</summary>
	public static SpringFeel Default => new SpringFeel(3f, 0.6f);

	/// <summary>Settles without overshooting, halfway to its goal in <paramref name="halfLife"/> seconds.</summary>
	public static SpringFeel HalfLife(float halfLife)
	{
		var omega = 2f * MathF.Log(2f) / MathF.Max(halfLife, 1e-5f);
		return new SpringFeel(omega / MathF.Tau, 1f);
	}

	internal float Omega => MathF.Tau * MathF.Max(Frequency, 1e-4f);

	/// <summary>The velocity a goal's jump gives at once, for each unit it jumped.</summary>
	internal float Kick => Response * Damping * Omega;

	/// <summary>
	/// How a step carries an offset from the goal and a velocity, solved exactly so it moves the same at any frame rate:
	/// the offset after is a·offset + b·velocity, and the velocity after c·offset + d·velocity.
	/// </summary>
	internal (float A, float B, float C, float D) Step(float dt)
	{
		var (w, z, t) = (Omega, MathF.Max(Damping, 0f), MathF.Max(dt, 0f));
		if (MathF.Abs(z - 1f) < 1e-4f)
		{
			var e = MathF.Exp(-w * t);
			return (e * (1f + w * t), e * t, -e * w * w * t, e * (1f - w * t));
		}
		if (z < 1f)
		{
			var wd = w * MathF.Sqrt(1f - z * z);
			var (e, cos, sin) = (MathF.Exp(-z * w * t), MathF.Cos(wd * t), MathF.Sin(wd * t));
			return (e * (cos + z * w / wd * sin), e * sin / wd, -e * w * w / wd * sin, e * (cos - z * w / wd * sin));
		}

		var root = MathF.Sqrt(z * z - 1f);
		var (r1, r2) = (-w * (z - root), -w * (z + root));
		var (e1, e2) = (MathF.Exp(r1 * t), MathF.Exp(r2 * t));
		var gap = r1 - r2;
		return ((r1 * e2 - r2 * e1) / gap, (e1 - e2) / gap, r1 * r2 * (e2 - e1) / gap, (r1 * e1 - r2 * e2) / gap);
	}
}
