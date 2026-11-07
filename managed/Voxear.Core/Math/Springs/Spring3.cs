using System.Numerics;

namespace Voxear;

/// <summary>
/// A point or direction following a goal on a spring, as <see cref="Spring"/> does a number.
/// <code>
/// private Spring3 sway = new Spring3(Vector3.Zero, new SpringFeel(2.5f, 0.4f));
///
/// protected override void Update()
/// {
/// 	sway.Kick(new Vector3(-Mouse.Delta.X, Mouse.Delta.Y, 0f) * 0.002f);
/// 	gun.Transform.LocalPosition = rest + sway.Update(Vector3.Zero, Time.DeltaTime);
/// }
/// </code>
/// </summary>
public struct Spring3
{
	private Vector3 goal;

	public Spring3(Vector3 value, SpringFeel feel)
	{
		Value = value;
		goal = value;
		Feel = feel;
	}

	public Vector3 Value { get; private set; }

	public Vector3 Velocity { get; set; }

	public SpringFeel Feel { get; set; }

	/// <summary>Moves <paramref name="dt"/> seconds toward <paramref name="target"/>, and gives back where it is.</summary>
	public Vector3 Update(Vector3 target, float dt)
	{
		if (target != goal)
		{
			Velocity += (target - goal) * Feel.Kick;
			goal = target;
		}

		var (a, b, c, d) = Feel.Step(dt);
		var offset = Value - target;
		Value = target + offset * a + Velocity * b;
		Velocity = offset * c + Velocity * d;
		return Value;
	}

	/// <summary>Adds to its velocity at once, as a blow does.</summary>
	public void Kick(Vector3 velocity)
	{
		Velocity += velocity;
	}

	/// <summary>Puts it at <paramref name="value"/>, at rest.</summary>
	public void Snap(Vector3 value)
	{
		(Value, goal, Velocity) = (value, value, Vector3.Zero);
	}
}
