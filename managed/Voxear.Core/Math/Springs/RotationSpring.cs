using System;
using System.Numerics;

namespace Voxear;

/// <summary>
/// A turn following a goal on a spring, as <see cref="Spring"/> does a number: it lags, overshoots and settles.
/// <code>
/// private RotationSpring aim = new RotationSpring(Quaternion.Identity, new SpringFeel(4f, 0.5f));
///
/// protected override void Update()
/// {
/// 	Transform.Rotation = aim.Update(target.Transform.Rotation, Time.DeltaTime);
/// }
/// </code>
/// </summary>
public struct RotationSpring
{
	private Spring3 offset;

	private Vector3 velocity;

	public RotationSpring(Quaternion value, SpringFeel feel)
	{
		Value = value;
		offset = new Spring3(Vector3.Zero, feel);
	}

	public Quaternion Value { get; private set; }

	public SpringFeel Feel
	{
		readonly get { return offset.Feel; }
		set { offset.Feel = value; }
	}

	/// <summary>Moves <paramref name="dt"/> seconds toward <paramref name="target"/>, and gives back where it is.</summary>
	public Quaternion Update(Quaternion target, float dt)
	{
		var from = Quaternion.Normalize(Value * Quaternion.Inverse(target));
		if (from.W < 0f)
		{
			from = Quaternion.Negate(from);
		}

		offset.Snap(ScaledAxis(from));
		offset.Velocity = velocity;
		var turned = offset.Update(Vector3.Zero, dt);
		velocity = offset.Velocity;
		Value = Quaternion.Normalize(FromScaledAxis(turned) * target);
		return Value;
	}

	/// <summary>Spins it at once, by an axis scaled by radians a second.</summary>
	public void Kick(Vector3 spin)
	{
		velocity += spin;
	}

	/// <summary>Puts it at <paramref name="value"/>, at rest.</summary>
	public void Snap(Quaternion value)
	{
		Value = value;
		velocity = Vector3.Zero;
	}

	private static Vector3 ScaledAxis(Quaternion turn)
	{
		var sine = MathF.Sqrt(MathF.Max(1f - turn.W * turn.W, 0f));
		if (sine < 1e-6f)
		{
			return Vector3.Zero;
		}

		var angle = 2f * MathF.Acos(Math.Clamp(turn.W, -1f, 1f));
		return new Vector3(turn.X, turn.Y, turn.Z) / sine * angle;
	}

	private static Quaternion FromScaledAxis(Vector3 axis)
	{
		var angle = axis.Length();
		return angle < 1e-6f ? Quaternion.Identity : Quaternion.CreateFromAxisAngle(axis / angle, angle);
	}
}
