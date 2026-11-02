using System;
using System.Numerics;
using Voxear;

namespace Hello;

/// <summary>Drifts upward with a little wander, then destroys its own Obj.</summary>
public sealed class Firefly : Behaviour
{
	private const float WanderSpeed = 0.25f;

	[SerializeField] private float lifetime = 3f;
	[SerializeField] private float riseSpeed = 0.35f;

	/// <summary>Set in OnEnable, which runs before Instantiate returns.</summary>
	public bool Ready { get; private set; }

	private float age;
	private Vector3 wander;
	private bool leaving;

	protected override void Awake()
	{
		var drift = Random.InsideUnitCircle * WanderSpeed;
		wander = new Vector3(drift.X, 0f, drift.Y);
	}

	protected override void OnEnable()
	{
		Ready = true;
	}

	protected override void Update()
	{
		age += Time.Delta;
		Obj.Position += (Vector3.Up * riseSpeed + wander) * Time.Delta;
		if (age < lifetime || leaving)
		{
			return;
		}

		leaving = true;
		Obj.Destroy();
	}
}
