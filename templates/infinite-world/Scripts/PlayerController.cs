using System;
using System.Numerics;
using Voxear;

namespace {{Namespace}};

/// <summary>
/// Walks its Obj's Character Controller where the Move action points, as the camera sees it. Run goes faster, Jump
/// leaps for as long as it is held, and the Obj turns to face the way it goes. The maps follow it.
/// <para>
/// A world without end makes its ground round the camera as the game goes, and the ground's collision a little
/// after. The character starts on the highest ground at its place once that is made, and waits wherever the ground
/// under it is not made yet rather than fall through.
/// </para>
/// </summary>
public sealed class PlayerController : Behaviour
{
	// A push down while it stands keeps it on the ground down steps and slopes.
	private const float StandingFall = -2f;
	private const float SlowestTurning = 0.2f;
	private const float GroundSearch = 1000f;
	private const float StartSearch = 500f;
	private const float ChestHeight = 1f;
	private const float CollisionSlack = 0.5f;

	[SerializeField, Required, Tooltip("The camera the player is walked as seen from")] private Obj view;
	[SerializeField, Suffix("m/s")] private float walkSpeed = 4.5f;
	[SerializeField, Suffix("m/s")] private float runSpeed = 8f;
	[SerializeField, Suffix("m")] private float jumpHeight = 1.25f;
	[SerializeField, Suffix("m/s²")] private float acceleration = 40f;
	[SerializeField, Suffix("°/s")] private float turnSpeed = 720f;

	private InputAction move = null!;
	private InputAction run = null!;
	private InputAction jump = null!;
	private Vector3 across;
	private float falling;
	private bool started;

	protected override void Start()
	{
		move = Actions.Player.Move;
		run = Actions.Player.Run;
		jump = Actions.Player.Jump;
		Map.Follow = Obj;
		// Until it stands on the ground it is no body, so the ray that looks for the ground does not meet it.
		Obj.SetEnabled(EngineComponent.CharacterController, false);
	}

	protected override void Update()
	{
		if (!started)
		{
			started = TryStandOnGround();
			return;
		}
		if (!TryFindGround(Obj.WorldPosition + Vector3.Up * ChestHeight, out _))
		{
			across = Vector3.Zero;
			return;
		}

		var controller = Obj.CharacterController;
		across = Vector3.MoveTowards(across, WantedVelocity(), acceleration * Time.Delta);
		falling = FallSpeed(controller);
		controller.Move((across + Vector3.Up * falling) * Time.Delta);
		StopRisingUnderRoof(controller);
		FaceTravel();
	}

	private bool TryStandOnGround()
	{
		if (!TryFindGround(Obj.WorldPosition + Vector3.Up * StartSearch, out var ground))
		{
			return false;
		}

		Obj.Position = ground;
		Obj.SetEnabled(EngineComponent.CharacterController, true);
		return true;
	}

	/// <summary>
	/// The ground under a point, once the world has made its blocks there and physics has their collision. A ray
	/// that starts inside the character passes out of it.
	/// </summary>
	private static bool TryFindGround(Vector3 from, out Vector3 ground)
	{
		ground = default;
		if (!Blocks.Raycast(from, Vector3.Down, out var block, GroundSearch))
		{
			return false;
		}
		if (!Physics.Raycast(from, Vector3.Down, out var hit, block.Distance + CollisionSlack))
		{
			return false;
		}

		ground = hit.Point;
		return true;
	}

	private Vector3 WantedVelocity()
	{
		var pushed = move.Vector;
		var direction = Flat(view.Right) * pushed.X + Flat(view.Forward) * pushed.Y;
		return direction * (run.Held ? runSpeed : walkSpeed);
	}

	private float FallSpeed(CharacterController controller)
	{
		var gravity = Physics.Gravity.Y;
		if (!controller.IsGrounded)
		{
			return falling + gravity * Time.Delta;
		}
		return jump.Held ? MathF.Sqrt(-2f * gravity * jumpHeight) : StandingFall;
	}

	private void StopRisingUnderRoof(CharacterController controller)
	{
		if (falling > 0f && controller.Velocity.Y <= 0f)
		{
			falling = 0f;
		}
	}

	private void FaceTravel()
	{
		if (across.Length() < SlowestTurning)
		{
			return;
		}

		var facing = Quaternion.LookRotation(across);
		Obj.Rotation = Quaternion.RotateTowards(Obj.Rotation, facing, turnSpeed * Time.Delta);
	}

	private static Vector3 Flat(Vector3 direction)
	{
		return direction.WithY(0f).Normalized;
	}
}
