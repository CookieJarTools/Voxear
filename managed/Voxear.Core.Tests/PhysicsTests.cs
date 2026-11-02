using System.Numerics;
using Voxear.Scripting;

namespace Voxear.Tests;

/// <summary>The physics API with no engine behind it: nothing has physics, every question finds nothing, and nothing throws.</summary>
internal static class PhysicsTests
{
	[Test]
	private static void AnObjWithoutPhysicsReadsAsTheDefaults()
	{
		var collider = Obj.None.Collider;
		Check.True(!collider.IsValid && collider == Collider.None, "no Collider on None");
		Check.True(collider.Shape == ColliderShape.Box && !collider.IsTrigger, "a box, solid");
		Check.Near(0.6f, collider.Friction);
		var body = Obj.None.Rigidbody;
		Check.True(!body.IsValid && !body.IsKinematic && !body.IsSleeping, "no Rigidbody");
		Check.Near(1f, body.Mass);
		Check.True(body.Velocity == Vector3.Zero, "still");
		body.AddForce(Vector3.UnitY, ForceMode.Impulse);
		body.AddTorque(Vector3.UnitY, ForceMode.VelocityChange);
		var walker = Obj.None.CharacterController;
		Check.True(!walker.IsValid && !walker.IsGrounded && walker.GroundNormal == Vector3.UnitY, "no character");
		walker.Move(Vector3.UnitZ);
	}

	[Test]
	private static void QuestionsFindNothingWithoutTheEngine()
	{
		Check.True(Physics.Gravity == new Vector3(0f, -9.81f, 0f), "gravity starts down");
		Check.True(!Physics.Raycast(Vector3.Zero, Vector3.UnitX, out var hit) && hit.Obj == Obj.None, "no ray meets anything");
		Check.True(Physics.OverlapSphere(Vector3.Zero, 10f).Length == 0 && !Physics.CheckSphere(Vector3.Zero, 1f), "nor any sphere");
	}

	[Test]
	private static void PhysicsComponentsAreFieldsOfOtherObjs()
	{
		Check.True(typeof(IObjComponent).IsAssignableFrom(typeof(Rigidbody)), "a Rigidbody field holds another Obj's");
		Check.True(typeof(IObjComponent).IsAssignableFrom(typeof(Collider)), "and a Collider field another's Collider");
		Check.True(typeof(IObjComponent).IsAssignableFrom(typeof(CharacterController)), "and so on");
	}
}
