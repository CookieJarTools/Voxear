using System;
using System.Numerics;

namespace Voxear
{
	/// <summary>
	/// Unity's Quaternion helpers on System.Numerics.Quaternion. Angles are in degrees and Euler angles apply Z, then
	/// X, then Y, as in Unity and in scene files. LookRotation is Voxear's own: the Matrix4x4 look-at helpers in .NET
	/// are right-handed, and would face the wrong way here.
	/// </summary>
	public static class QuaternionExtensions
	{
		extension(Quaternion)
		{
			/// <summary>Rotates a vector, as rotation * vector does in Unity.</summary>
			public static Vector3 operator *(Quaternion rotation, Vector3 vector)
			{
				return Vector3.Transform(vector, rotation);
			}

			/// <summary>A rotation from Euler angles in degrees: Z, then X, then Y.</summary>
			public static Quaternion Euler(float x, float y, float z)
			{
				return Quaternion.CreateFromYawPitchRoll(y * Mathf.Deg2Rad, x * Mathf.Deg2Rad, z * Mathf.Deg2Rad);
			}

			public static Quaternion Euler(Vector3 degrees)
			{
				return Quaternion.Euler(degrees.X, degrees.Y, degrees.Z);
			}

			/// <summary>A rotation of some degrees around an axis. Positive turns right around Up.</summary>
			public static Quaternion AngleAxis(float degrees, Vector3 axis)
			{
				return Quaternion.CreateFromAxisAngle(axis.Normalized, degrees * Mathf.Deg2Rad);
			}

			/// <summary>The rotation that faces forward, with up as close to Vector3.Up as it can be.</summary>
			public static Quaternion LookRotation(Vector3 forward)
			{
				return Quaternion.LookRotation(forward, Vector3.Up);
			}

			/// <summary>
			/// The rotation that faces forward, with its up as close to the given up as it can be. A zero forward gives
			/// Identity; a forward parallel to up picks a sideways direction rather than failing.
			/// </summary>
			public static Quaternion LookRotation(Vector3 forward, Vector3 up)
			{
				var z = forward.Normalized;
				if (z == Vector3.Zero)
				{
					return Quaternion.Identity;
				}

				var x = Vector3.Cross(up, z).Normalized;
				if (x == Vector3.Zero)
				{
					x = Vector3.Cross(MathF.Abs(z.Y) < 0.9f ? Vector3.Up : Vector3.Forward, z).Normalized;
				}

				var y = Vector3.Cross(z, x);
				// Matrix4x4 rows are where the X, Y and Z axes end up.
				var basis = new Matrix4x4(x.X, x.Y, x.Z, 0f, y.X, y.Y, y.Z, 0f, z.X, z.Y, z.Z, 0f, 0f, 0f, 0f, 1f);
				return Quaternion.Normalize(Quaternion.CreateFromRotationMatrix(basis));
			}

			/// <summary>The shortest rotation that turns one direction into another.</summary>
			public static Quaternion FromToRotation(Vector3 from, Vector3 to)
			{
				var a = from.Normalized;
				var b = to.Normalized;
				var dot = Vector3.Dot(a, b);
				if (dot > 0.999999f)
				{
					return Quaternion.Identity;
				}
				if (dot < -0.999999f)
				{
					return Quaternion.CreateFromAxisAngle(AnyPerpendicular(a), MathF.PI);
				}
				return Quaternion.Normalize(new Quaternion(Vector3.Cross(a, b), 1f + dot));
			}

			/// <summary>Turns from towards to by at most maxDegrees, without overshooting.</summary>
			public static Quaternion RotateTowards(Quaternion from, Quaternion to, float maxDegrees)
			{
				var angle = Quaternion.Angle(from, to);
				return angle <= maxDegrees || angle == 0f ? to : Quaternion.Slerp(from, to, maxDegrees / angle);
			}

			/// <summary>The angle between two rotations, from 0 to 180 degrees.</summary>
			public static float Angle(Quaternion a, Quaternion b)
			{
				var dot = MathF.Min(MathF.Abs(Quaternion.Dot(a, b)), 1f);
				return 2f * MathF.Acos(dot) * Mathf.Rad2Deg;
			}
		}

		extension(Quaternion rotation)
		{
			/// <summary>Euler angles in degrees, each from 0 to 360, as Unity's eulerAngles.</summary>
			public Vector3 EulerAngles => Wrap(ToEuler(rotation));

			public Vector3 Forward => Vector3.Transform(Vector3.Forward, rotation);

			public Vector3 Right => Vector3.Transform(Vector3.Right, rotation);

			public Vector3 Up => Vector3.Transform(Vector3.Up, rotation);
		}

		private static Vector3 AnyPerpendicular(Vector3 direction)
		{
			var axis = Vector3.Cross(Vector3.Right, direction);
			return axis.LengthSquared() > 1e-6f ? Vector3.Normalize(axis) : Vector3.Normalize(Vector3.Cross(Vector3.Up, direction));
		}

		/// <summary>
		/// Undoes Euler: the rotation is Ry · Rx · Rz, so its matrix's row 1, column 2 holds -sin(x), and the other
		/// angles come from the entries beside it. Facing straight up or down, Y absorbs Z, which is set to zero.
		/// </summary>
		private static Vector3 ToEuler(Quaternion q)
		{
			var m12 = 2f * (q.Y * q.Z - q.X * q.W);
			var x = MathF.Asin(Mathf.Clamp(-m12, -1f, 1f));
			if (MathF.Abs(m12) > 0.99999f)
			{
				var m20 = 2f * (q.X * q.Z - q.Y * q.W);
				var m00 = 1f - 2f * (q.Y * q.Y + q.Z * q.Z);
				return new Vector3(x, MathF.Atan2(-m20, m00), 0f) * Mathf.Rad2Deg;
			}

			var m02 = 2f * (q.X * q.Z + q.Y * q.W);
			var m22 = 1f - 2f * (q.X * q.X + q.Y * q.Y);
			var m10 = 2f * (q.X * q.Y + q.Z * q.W);
			var m11 = 1f - 2f * (q.X * q.X + q.Z * q.Z);
			return new Vector3(x, MathF.Atan2(m02, m22), MathF.Atan2(m10, m11)) * Mathf.Rad2Deg;
		}

		private static Vector3 Wrap(Vector3 degrees)
		{
			return new Vector3(Mathf.Repeat(degrees.X, 360f), Mathf.Repeat(degrees.Y, 360f), Mathf.Repeat(degrees.Z, 360f));
		}
	}
}
