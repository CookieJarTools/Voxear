using System;
using System.Numerics;

namespace Voxear
{
	/// <summary>Unity's Vector2 helpers on System.Numerics.Vector2, for 2D games and UI.</summary>
	public static class Vector2Extensions
	{
		extension(Vector2)
		{
			public static Vector2 Up => Vector2.UnitY;
			public static Vector2 Down => -Vector2.UnitY;
			public static Vector2 Right => Vector2.UnitX;
			public static Vector2 Left => -Vector2.UnitX;

			/// <summary>Moves current towards target by at most maxDistance, without overshooting.</summary>
			public static Vector2 MoveTowards(Vector2 current, Vector2 target, float maxDistance)
			{
				var offset = target - current;
				var distance = offset.Length();
				return distance <= maxDistance || distance == 0f ? target : current + offset / distance * maxDistance;
			}

			/// <summary>The angle between two directions, from 0 to 180 degrees.</summary>
			public static float Angle(Vector2 from, Vector2 to)
			{
				var lengths = MathF.Sqrt(from.LengthSquared() * to.LengthSquared());
				return lengths < 1e-15f ? 0f : MathF.Acos(Mathf.Clamp(Vector2.Dot(from, to) / lengths, -1f, 1f)) * Mathf.Rad2Deg;
			}

			/// <summary>The angle from one direction to another, from -180 to 180 degrees. Anticlockwise is positive.</summary>
			public static float SignedAngle(Vector2 from, Vector2 to)
			{
				return Vector2.Angle(from, to) * MathF.Sign(from.X * to.Y - from.Y * to.X);
			}

			/// <summary>vector shortened to maxLength when it is longer.</summary>
			public static Vector2 ClampMagnitude(Vector2 vector, float maxLength)
			{
				var length = vector.LengthSquared();
				return length > maxLength * maxLength ? vector / MathF.Sqrt(length) * maxLength : vector;
			}

			/// <summary>vector turned a quarter anticlockwise.</summary>
			public static Vector2 Perpendicular(Vector2 vector)
			{
				return new Vector2(-vector.Y, vector.X);
			}
		}

		extension(Vector2 vector)
		{
			public float Magnitude => vector.Length();

			public float SqrMagnitude => vector.LengthSquared();

			/// <summary>The direction with length 1, or zero for a zero vector.</summary>
			public Vector2 Normalized
			{
				get
				{
					var length = vector.Length();
					return length > 1e-5f ? vector / length : Vector2.Zero;
				}
			}
		}
	}
}
