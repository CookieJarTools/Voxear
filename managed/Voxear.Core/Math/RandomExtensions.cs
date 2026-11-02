using System;
using System.Numerics;

namespace Voxear;

/// <summary>
/// Unity's Random helpers on System.Random, so there is no second Random type to clash with. Random.Range and the
/// rest use the shared generator. For repeatable results, make your own seeded one and use the Next helpers on
/// it, as in new Random(42).NextFloat(0f, 10f).
/// </summary>
public static class RandomExtensions
{
	extension(Random)
	{
		/// <summary>A float from min up to, but not including, max.</summary>
		public static float Range(float min, float max)
		{
			return Random.Shared.NextFloat(min, max);
		}

		/// <summary>An int from min up to, but not including, max, as Unity's int Range.</summary>
		public static int Range(int min, int max)
		{
			return Random.Shared.Next(min, max);
		}

		/// <summary>A float from 0 up to, but not including, 1.</summary>
		public static float Value => Random.Shared.NextSingle();

		/// <summary>True with the given probability, from 0 to 1.</summary>
		public static bool Chance(float probability)
		{
			return Random.Shared.NextBool(probability);
		}

		public static Vector3 InsideUnitSphere => Random.Shared.NextInSphere();

		public static Vector3 OnUnitSphere => Random.Shared.NextOnSphere();

		public static Vector2 InsideUnitCircle => Random.Shared.NextInCircle();

		/// <summary>A rotation where every orientation is equally likely.</summary>
		public static Quaternion Rotation => Random.Shared.NextRotation();
	}

	extension(Random random)
	{
		/// <summary>A float from min up to, but not including, max.</summary>
		public float NextFloat(float min, float max)
		{
			return min + random.NextSingle() * (max - min);
		}

		/// <summary>True with the given probability, from 0 to 1.</summary>
		public bool NextBool(float probability)
		{
			return random.NextSingle() < probability;
		}

		/// <summary>A point inside the unit sphere, spread evenly through its volume.</summary>
		public Vector3 NextInSphere()
		{
			return random.NextOnSphere() * MathF.Cbrt(random.NextSingle());
		}

		/// <summary>A direction where every direction is equally likely.</summary>
		public Vector3 NextOnSphere()
		{
			var z = random.NextSingle() * 2f - 1f;
			var angle = random.NextSingle() * MathF.Tau;
			var radius = MathF.Sqrt(1f - z * z);
			return new Vector3(radius * MathF.Cos(angle), radius * MathF.Sin(angle), z);
		}

		/// <summary>A point inside the unit circle, spread evenly over its area.</summary>
		public Vector2 NextInCircle()
		{
			var angle = random.NextSingle() * MathF.Tau;
			return new Vector2(MathF.Cos(angle), MathF.Sin(angle)) * MathF.Sqrt(random.NextSingle());
		}

		/// <summary>A rotation where every orientation is equally likely (Shoemake's method).</summary>
		public Quaternion NextRotation()
		{
			var u = random.NextSingle();
			var a = random.NextSingle() * MathF.Tau;
			var b = random.NextSingle() * MathF.Tau;
			var s = MathF.Sqrt(1f - u);
			var t = MathF.Sqrt(u);
			return new Quaternion(s * MathF.Sin(a), s * MathF.Cos(a), t * MathF.Sin(b), t * MathF.Cos(b));
		}
	}
}
