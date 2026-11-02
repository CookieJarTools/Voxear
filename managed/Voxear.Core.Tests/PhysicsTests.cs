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
	private static void TwoDimensionalPhysicsReadsAsTheDefaultsToo()
	{
		var collider = Obj.None.Collider2D;
		Check.True(!collider.IsValid && collider.Shape == ColliderShape2D.Box && collider.Size == Vector2.One, "no Collider2D");
		var body = Obj.None.Rigidbody2D;
		Check.True(!body.IsValid && !body.FixedRotation && body.Velocity == Vector2.Zero, "no Rigidbody2D");
		Check.Near(1f, body.GravityScale);
		body.AddForce(Vector2.UnitX, ForceMode2D.Impulse);
		Check.True(Physics2D.Gravity == new Vector2(0f, -9.81f), "gravity starts down");
		Check.True(!Physics2D.Raycast(Vector2.Zero, Vector2.UnitX, out _) && Physics2D.OverlapCircle(Vector2.Zero, 1f).Length == 0, "nothing is found");
	}

	[Test]
	private static void AJointReadsAsHeldToTheWorldWithoutTheEngine()
	{
		var joint = Obj.None.Joint;
		Check.True(!joint.IsValid && joint.Type == JointType.Fixed && !joint.UseLimits, "no Joint");
		Check.True(!joint.ConnectedBody.IsValid && joint.Axis == Vector3.UnitY, "held to the world, about Y");
		joint.SetLimits(0f, 90f);
		joint.ConnectedBody = Rigidbody.None;
	}

	[Test]
	private static void LayerMasksHoldLayersByNumber()
	{
		Check.True(Obj.None.Layer == 0 && LayerMask.LayerToName(0) == "Default", "Default without the engine");
		Check.True(LayerMask.NameToLayer("Nobody") == -1 && LayerMask.GetMask("Default", "Nobody").Value == 1, "unknown names are left out");
		LayerMask mask = (1 << 3) | (1 << 5);
		Check.True(mask.Contains(5) && !mask.Contains(4) && !mask.Contains(40), "by bit");
		Check.True(LayerMask.Everything.Contains(31) && !LayerMask.Nothing.Contains(0), "every one, and none");
	}

	[Test]
	private static void PhysicsComponentsAreFieldsOfOtherObjs()
	{
		Check.True(typeof(IObjComponent).IsAssignableFrom(typeof(Rigidbody)), "a Rigidbody field holds another Obj's");
		Check.True(typeof(IObjComponent).IsAssignableFrom(typeof(Collider)), "and a Collider field another's Collider");
		Check.True(typeof(IObjComponent).IsAssignableFrom(typeof(CharacterController)), "and so on");
	}
}
