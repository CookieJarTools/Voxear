using System.Collections.Generic;
using System.Numerics;
using Voxear;

namespace Wasteland;

/// <summary>
/// Walks a soldier round its points, running to every other one. At each point it may stop, turn to its target,
/// aim and fire a few shots before going on. Its Mob Motion walks, runs and idles it as it moves; aiming and firing
/// are the clips in its model, played on its Animator.
/// </summary>
public sealed class Patrol : Behaviour
{
	[SerializeField] private List<Vector3> points = [];
	[SerializeField] private float walkSpeed = 1.3f;
	[SerializeField] private float runSpeed = 3.2f;
	[SerializeField] private float degreesPerSecond = 360f;
	[SerializeField] private float waitSeconds = 1f;
	[SerializeField] private float aimSeconds = 2.4f;
	[SerializeField] private int shots = 2;
	[SerializeField] private bool aimAtEveryPoint;
	[SerializeField] private Vector3 target;
	[SerializeField] private AnimationClip aim;
	[SerializeField] private AnimationClip fire;

	private int next;
	private float stopped = -1f;
	private int fired;
	private float lastShot;

	protected override void Update()
	{
		if (points.Count == 0)
		{
			return;
		}
		if (stopped >= 0f)
		{
			Stand();
			return;
		}
		var goal = points[next].WithY(Obj.Position.Y);
		var way = goal - Obj.Position;
		var step = (next % 2 == 1 ? runSpeed : walkSpeed) * Time.Delta;
		if (way.Length() <= step)
		{
			Obj.Position = goal;
			stopped = 0f;
			fired = 0;
			return;
		}
		Face(way);
		Obj.Position += Vector3.Normalize(way) * step;
	}

	/// <summary>At a point: aiming at the target, firing now and then, or only waiting, then on to the next.</summary>
	private void Stand()
	{
		stopped += Time.Delta;
		var aiming = aimAtEveryPoint || next % 2 == 0;
		var animator = Obj.Animator;
		if (!aiming || animator is null)
		{
			if (stopped >= waitSeconds)
			{
				GoOn();
			}
			return;
		}
		Face(target - Obj.Position);
		var raise = 0.4f;
		if (stopped < raise)
		{
			if (animator.Clip != aim)
			{
				animator.Play(aim);
			}
			return;
		}
		// Shots spread over the time aiming, each a kick back into the aim.
		var shotAt = raise + (fired + 1) * (aimSeconds - raise) / (shots + 1);
		if (fired < shots && stopped >= shotAt)
		{
			animator.Play(fire, looping: false);
			fired++;
			lastShot = stopped;
		}
		else if (animator.Clip == fire && stopped - lastShot >= 0.4f)
		{
			animator.Play(aim);
		}
		if (stopped >= aimSeconds)
		{
			animator.Stop();
			GoOn();
		}
	}

	private void GoOn()
	{
		stopped = -1f;
		next = (next + 1) % points.Count;
	}

	private void Face(Vector3 way)
	{
		var flat = way.WithY(0f);
		if (flat.LengthSquared() < 1e-6f)
		{
			return;
		}
		Obj.Rotation = Quaternion.RotateTowards(Obj.Rotation, Quaternion.LookRotation(flat), degreesPerSecond * Time.Delta);
	}
}
