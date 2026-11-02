using System.Numerics;
using Voxear;

namespace Hello;

/// <summary>Turns its Obj, at a steady rate and only around the vertical, until it faces a point.</summary>
public sealed class Turner : Behaviour
{
	[SerializeField] private Vector3 target;
	[SerializeField] private float degreesPerSecond = 30f;

	protected override void Update()
	{
		var flat = (target - Obj.WorldPosition).WithY(0f);
		var facing = Quaternion.LookRotation(flat);
		Obj.Rotation = Quaternion.RotateTowards(Obj.Rotation, facing, degreesPerSecond * Time.Delta);
	}
}
