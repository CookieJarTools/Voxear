using System;
using System.Numerics;

namespace Voxear;

/// <summary>
/// Unity's Vector3 helpers on System.Numerics.Vector3, so Vector3.Forward and Vector3.MoveTowards read as they do
/// in Unity while scripts keep the standard type. Directions follow Unity: Y up, Z forward, X right.
/// </summary>
public static class Vector3Extensions
{
	extension(Vector3)
	{
		public static Vector3 Forward => Vector3.UnitZ;
		public static Vector3 Back => -Vector3.UnitZ;
		public static Vector3 Up => Vector3.UnitY;
		public static Vector3 Down => -Vector3.UnitY;
		public static Vector3 Right => Vector3.UnitX;
		public static Vector3 Left => -Vector3.UnitX;

		/// <summary>Moves current towards target by at most maxDistance, without overshooting.</summary>
		public static Vector3 MoveTowards(Vector3 current, Vector3 target, float maxDistance)
		{
			var offset = target - current;
			var distance = offset.Length();
			return distance <= maxDistance || distance == 0f ? target : current + offset / distance * maxDistance;
		}

		/// <summary>Eases towards target the same way at any frame rate. See Mathf.Damp.</summary>
		public static Vector3 Damp(Vector3 current, Vector3 target, float smoothing, float deltaTime)
		{
			return Vector3.Lerp(target, current, MathF.Exp(-deltaTime * 4.6f / MathF.Max(smoothing, 1e-4f)));
		}

		/// <summary>Follows target like a spring that never overshoots. See Mathf.SmoothDamp.</summary>
		public static Vector3 SmoothDamp(Vector3 current, Vector3 target, ref Vector3 velocity, float smoothTime)
		{
			return Vector3.SmoothDamp(current, target, ref velocity, smoothTime, float.PositiveInfinity, Time.Delta);
		}

		public static Vector3 SmoothDamp(Vector3 current, Vector3 target, ref Vector3 velocity, float smoothTime, float maxSpeed, float deltaTime)
		{
			var x = Mathf.SmoothDamp(current.X, target.X, ref velocity.X, smoothTime, maxSpeed, deltaTime);
			var y = Mathf.SmoothDamp(current.Y, target.Y, ref velocity.Y, smoothTime, maxSpeed, deltaTime);
			var z = Mathf.SmoothDamp(current.Z, target.Z, ref velocity.Z, smoothTime, maxSpeed, deltaTime);
			return new Vector3(x, y, z);
		}

		/// <summary>The angle between two directions, from 0 to 180 degrees.</summary>
		public static float Angle(Vector3 from, Vector3 to)
		{
			var lengths = MathF.Sqrt(from.LengthSquared() * to.LengthSquared());
			return lengths < 1e-15f ? 0f : MathF.Acos(Mathf.Clamp(Vector3.Dot(from, to) / lengths, -1f, 1f)) * Mathf.Rad2Deg;
		}

		/// <summary>
		/// The angle from one direction to another around an axis, from -180 to 180 degrees. Turning right about Up
		/// is positive, as in Unity.
		/// </summary>
		public static float SignedAngle(Vector3 from, Vector3 to, Vector3 axis)
		{
			return Vector3.Angle(from, to) * MathF.Sign(Vector3.Dot(axis, Vector3.Cross(from, to)));
		}

		/// <summary>The part of vector that points along onNormal.</summary>
		public static Vector3 Project(Vector3 vector, Vector3 onNormal)
		{
			var length = onNormal.LengthSquared();
			return length < 1e-15f ? Vector3.Zero : onNormal * (Vector3.Dot(vector, onNormal) / length);
		}

		/// <summary>vector flattened onto a plane, such as a direction along the ground.</summary>
		public static Vector3 ProjectOnPlane(Vector3 vector, Vector3 planeNormal)
		{
			return vector - Vector3.Project(vector, planeNormal);
		}

		/// <summary>vector shortened to maxLength when it is longer.</summary>
		public static Vector3 ClampMagnitude(Vector3 vector, float maxLength)
		{
			var length = vector.LengthSquared();
			return length > maxLength * maxLength ? vector / MathF.Sqrt(length) * maxLength : vector;
		}
	}

	extension(Vector3 vector)
	{
		public float Magnitude => vector.Length();

		public float SqrMagnitude => vector.LengthSquared();

		/// <summary>The direction with length 1, or zero for a zero vector, where Vector3.Normalize gives NaN.</summary>
		public Vector3 Normalized
		{
			get
			{
				var length = vector.Length();
				return length > 1e-5f ? vector / length : Vector3.Zero;
			}
		}

		public Vector3 WithX(float x)
		{
			return vector with { X = x };
		}

		public Vector3 WithY(float y)
		{
			return vector with { Y = y };
		}

		public Vector3 WithZ(float z)
		{
			return vector with { Z = z };
		}
	}
}
