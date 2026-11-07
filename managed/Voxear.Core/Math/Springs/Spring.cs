namespace Voxear;

/// <summary>
/// A number following a goal on a spring, with the overshoot and lag that make procedural motion look alive. It moves
/// the same at any frame rate.
/// <code>
/// private Spring lean = new Spring(0f, new SpringFeel(2f, 0.5f));
///
/// protected override void Update()
/// {
/// 	var angle = lean.Update(Input.Value("Strafe") * 8f, Time.DeltaTime);
/// }
/// </code>
/// </summary>
public struct Spring
{
	private float goal;

	public Spring(float value, SpringFeel feel)
	{
		Value = value;
		goal = value;
		Feel = feel;
	}

	public float Value { get; private set; }

	public float Velocity { get; set; }

	public SpringFeel Feel { get; set; }

	/// <summary>Moves <paramref name="dt"/> seconds toward <paramref name="target"/>, and gives back where it is.</summary>
	public float Update(float target, float dt)
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
	public void Kick(float velocity)
	{
		Velocity += velocity;
	}

	/// <summary>Puts it at <paramref name="value"/>, at rest.</summary>
	public void Snap(float value)
	{
		(Value, goal, Velocity) = (value, value, 0f);
	}
}
