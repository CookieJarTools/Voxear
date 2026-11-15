using System;
using System.Numerics;
using Voxear;

namespace {{Namespace}};

/// <summary>
/// Keeps its Obj's camera behind a target and turns it round the target with the Look action. Zoom moves it nearer
/// and further, and it comes in front of whatever stands between it and the target, so it never looks from under
/// the ground. The pointer is held while the game plays: Free Pointer lets it go, and Hold Pointer takes it back.
/// While the game plays it takes over from the camera on Replaces, which is switched off so this one is the view.
/// </summary>
public sealed class OrbitCamera : Behaviour
{
	private const float Clearance = 0.3f;
	private const float NearestReach = 0.6f;

	[SerializeField, Required] private Obj target;
	[SerializeField, Tooltip("A camera of the level this one takes over from while the game plays")] private Obj replaces;
	[SerializeField, Suffix("m")] private float height = 1.5f;
	[SerializeField, Suffix("m")] private float distance = 6f;
	[SerializeField, Suffix("m")] private float nearest = 2f;
	[SerializeField, Suffix("m")] private float furthest = 16f;
	[SerializeField, Suffix("m")] private float zoomStep = 1f;
	[SerializeField, Suffix("m/s")] private float zoomSpeed = 8f;
	[SerializeField, Suffix("°")] private float lowest = -35f;
	[SerializeField, Suffix("°")] private float highest = 80f;
	[SerializeField, Suffix("°/pixel")] private float mouseSpeed = 0.12f;
	[SerializeField, Suffix("°/s")] private float stickSpeed = 160f;
	[SerializeField, Suffix("s")] private float heightSmoothing = 0.15f;

	private InputAction look = null!;
	private InputAction zoom = null!;
	private InputAction freePointer = null!;
	private InputAction holdPointer = null!;
	private float turn;
	private float tilt;
	private float followedHeight;
	private bool holdsPointer;

	protected override void Start()
	{
		if (replaces.IsValid)
		{
			replaces.SetEnabled(EngineComponent.Camera, false);
		}

		look = Actions.Player.Look;
		zoom = Actions.Player.Zoom;
		freePointer = Actions.Player.FreePointer;
		holdPointer = Actions.Player.HoldPointer;

		var angles = Obj.Rotation.EulerAngles;
		turn = angles.Y;
		tilt = Mathf.Clamp(Mathf.DeltaAngle(0f, angles.X), lowest, highest);
		followedHeight = target.WorldPosition.Y;
		HoldPointer(true);
	}

	protected override void LateUpdate()
	{
		if (freePointer.Pressed)
		{
			HoldPointer(false);
		}
		else if (holdPointer.Pressed)
		{
			HoldPointer(true);
		}

		Turn();
		Zoom();
		Follow();
	}

	private void HoldPointer(bool hold)
	{
		holdsPointer = hold;
		Cursor.Lock = hold ? CursorLock.Locked : CursorLock.None;
	}

	private void Turn()
	{
		var pushed = look.Vector;
		var byStick = Input.Scheme == ControlScheme.Gamepad;
		if (!byStick && !holdsPointer)
		{
			return;
		}

		// The mouse gives the pixels it moved this frame, down being more; a stick how far it is pushed, up being more.
		var degrees = byStick ? new Vector2(pushed.X, -pushed.Y) * stickSpeed * Time.Delta : pushed * mouseSpeed;
		turn = Mathf.Repeat(turn + degrees.X, 360f);
		tilt = Mathf.Clamp(tilt + degrees.Y, lowest, highest);
	}

	private void Zoom()
	{
		var pushed = zoom.Vector.Y;
		// The wheel gives the notches it turned this frame; a pad's buttons are held.
		var nearer = Input.Scheme == ControlScheme.Gamepad ? pushed * zoomSpeed * Time.Delta : pushed * zoomStep;
		distance = Mathf.Clamp(distance - nearer, nearest, furthest);
	}

	private void Follow()
	{
		var feet = target.WorldPosition;
		// A step climbed lifts the target at once; the camera rises after it.
		followedHeight = Mathf.Damp(followedHeight, feet.Y, heightSmoothing, Time.Delta);
		var pivot = feet.WithY(followedHeight + height);
		var rotation = Quaternion.Euler(tilt, turn, 0f);
		var back = -rotation.Forward;

		Obj.Position = pivot + back * Reach(pivot, back);
		Obj.Rotation = rotation;
	}

	private float Reach(Vector3 pivot, Vector3 back)
	{
		var blocked = Physics.Linecast(pivot, pivot + back * (distance + Clearance), out var hit) && hit.Obj != target;
		return blocked ? MathF.Max(hit.Distance - Clearance, NearestReach) : distance;
	}
}
