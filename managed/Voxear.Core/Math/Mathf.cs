using System;

namespace Voxear;

/// <summary>
/// Unity's Mathf, for the helpers System.MathF does not have: clamping, remapping, wrapping, angles and smoothing.
/// Angles are in degrees, as everywhere in Voxear's script API.
/// </summary>
public static class Mathf
{
	public const float PI = MathF.PI;
	public const float Tau = MathF.Tau;
	public const float Deg2Rad = MathF.PI / 180f;
	public const float Rad2Deg = 180f / MathF.PI;

	public static float Clamp(float value, float min, float max)
	{
		return value < min ? min : value > max ? max : value;
	}

	public static float Clamp01(float value)
	{
		return Clamp(value, 0f, 1f);
	}

	/// <summary>From a to b as t goes from 0 to 1. t is clamped, as in Unity.</summary>
	public static float Lerp(float a, float b, float t)
	{
		return a + (b - a) * Clamp01(t);
	}

	public static float LerpUnclamped(float a, float b, float t)
	{
		return a + (b - a) * t;
	}

	/// <summary>Where value lies between a and b, from 0 to 1. The inverse of Lerp.</summary>
	public static float InverseLerp(float a, float b, float value)
	{
		return a == b ? 0f : Clamp01((value - a) / (b - a));
	}

	/// <summary>Maps value from one range onto another, for example a speed onto a pitch. Not clamped.</summary>
	public static float Remap(float value, float fromMin, float fromMax, float toMin, float toMax)
	{
		var t = fromMin == fromMax ? 0f : (value - fromMin) / (fromMax - fromMin);
		return LerpUnclamped(toMin, toMax, t);
	}

	/// <summary>Like Lerp, but easing in and out.</summary>
	public static float SmoothStep(float from, float to, float t)
	{
		t = Clamp01(t);
		return LerpUnclamped(from, to, t * t * (3f - 2f * t));
	}

	/// <summary>Moves current towards target by at most maxDelta, without overshooting.</summary>
	public static float MoveTowards(float current, float target, float maxDelta)
	{
		return MathF.Abs(target - current) <= maxDelta ? target : current + MathF.Sign(target - current) * maxDelta;
	}

	/// <summary>Wraps t into [0, length), for looping values. Unlike %, it never goes negative.</summary>
	public static float Repeat(float t, float length)
	{
		return Clamp(t - MathF.Floor(t / length) * length, 0f, length);
	}

	/// <summary>Bounces t back and forth between 0 and length.</summary>
	public static float PingPong(float t, float length)
	{
		t = Repeat(t, length * 2f);
		return length - MathF.Abs(t - length);
	}

	/// <summary>The shortest difference between two angles, from -180 to 180 degrees.</summary>
	public static float DeltaAngle(float current, float target)
	{
		var delta = Repeat(target - current, 360f);
		return delta > 180f ? delta - 360f : delta;
	}

	/// <summary>Lerp for angles, taking the short way round.</summary>
	public static float LerpAngle(float a, float b, float t)
	{
		return a + DeltaAngle(a, b) * Clamp01(t);
	}

	/// <summary>MoveTowards for angles, taking the short way round.</summary>
	public static float MoveTowardsAngle(float current, float target, float maxDelta)
	{
		var delta = DeltaAngle(current, target);
		return -maxDelta < delta && delta < maxDelta ? target : MoveTowards(current, current + delta, maxDelta);
	}

	/// <summary>
	/// Eases towards target and keeps the same feel at any frame rate: smoothing is roughly how many seconds it
	/// takes to cover most of the distance. Simpler than SmoothDamp, with no velocity to keep.
	/// </summary>
	public static float Damp(float current, float target, float smoothing, float deltaTime)
	{
		return LerpUnclamped(target, current, MathF.Exp(-deltaTime * 4.6f / MathF.Max(smoothing, 1e-4f)));
	}

	/// <summary>
	/// Follows target like a spring that never overshoots, as Unity's SmoothDamp does. velocity carries the motion
	/// between calls, so keep it in a field. It reaches the target in about smoothTime seconds.
	/// </summary>
	public static float SmoothDamp(float current, float target, ref float velocity, float smoothTime)
	{
		return SmoothDamp(current, target, ref velocity, smoothTime, float.PositiveInfinity, Time.Delta);
	}

	public static float SmoothDamp(float current, float target, ref float velocity, float smoothTime, float maxSpeed, float deltaTime)
	{
		smoothTime = MathF.Max(0.0001f, smoothTime);
		var omega = 2f / smoothTime;
		var x = omega * deltaTime;
		// An approximation of e^-x that is exact enough here and cheap.
		var decay = 1f / (1f + x + 0.48f * x * x + 0.235f * x * x * x);
		var maxChange = maxSpeed * smoothTime;
		var change = Clamp(current - target, -maxChange, maxChange);
		var goal = current - change;
		var temp = (velocity + omega * change) * deltaTime;
		velocity = (velocity - omega * temp) * decay;
		var result = goal + (change + temp) * decay;
		if (target - current > 0f != result > target)
		{
			return result;
		}

		// It would have overshot, so it stops at the target.
		velocity = 0f;
		return target;
	}

	/// <summary>Whether two floats are equal apart from rounding error.</summary>
	public static bool Approximately(float a, float b)
	{
		return MathF.Abs(b - a) < MathF.Max(1e-6f * MathF.Max(MathF.Abs(a), MathF.Abs(b)), 1e-6f);
	}
}
