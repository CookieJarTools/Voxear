using System.Numerics;
using Voxear.Interop;
using Voxear.Scripting;

namespace Voxear;

/// <summary>
/// Script logic attached to an Obj, like a MonoBehaviour. The engine guarantees the order of the
/// lifecycle: every behaviour gets Awake, then OnEnable, then Start, and everything created together
/// finishes one phase before any of it begins the next. No Start runs while anything is still
/// waiting for its Awake or OnEnable.
/// </summary>
public abstract class Behaviour
{
	/// <summary>
	/// The Obj this behaviour is attached to. It is returned by reference so that Obj.Position = value
	/// works the way transform.position does in Unity.
	/// </summary>
	public ref readonly Obj Obj => ref obj;

	/// <summary>Takes effect between phases: OnDisable, or OnEnable followed by Start the first time.</summary>
	public bool Enabled
	{
		get { return enabled; }
		set { SetEnabled(value); }
	}

	/// <summary>Whether it is destroyed, or will be at the end of the frame.</summary>
	public bool IsDestroyed { get; private set; }

	internal uint Handle => handle;

	/// <summary>The type's ExecutionOrder, which orders behaviours within a phase.</summary>
	internal int Order { get; private set; }

	/// <summary>Field values from the scene or prefab, as JSON, waiting to be set before Awake.</summary>
	internal string? PendingFields { get; set; }

	private Obj obj;
	private uint handle;
	private bool enabled = true;

	/// <summary>Destroys this behaviour at the end of the frame, after OnDisable and OnDestroy.</summary>
	public void Destroy()
	{
		if (IsDestroyed)
		{
			return;
		}

		IsDestroyed = true;
		Native.Destroy(handle);
	}

	/// <summary>
	/// Called once, first, after the fields the scene or prefab gives it are set. Everything created with it has its
	/// Awake before any of it gets OnEnable, so this is the place to set up the behaviour itself.
	/// </summary>
	protected virtual void Awake()
	{
	}

	/// <summary>Called after Awake, and again each time the behaviour is enabled.</summary>
	protected virtual void OnEnable()
	{
	}

	/// <summary>
	/// Called once, before the first Update. Everything created with it has had Awake and OnEnable by then, so other
	/// behaviours are ready to be asked for what they hold.
	/// </summary>
	protected virtual void Start()
	{
	}

	/// <summary>Called at each fixed step, with physics, before Update. A frame can have none of them, or several.</summary>
	protected virtual void FixedUpdate()
	{
	}

	/// <summary>Called once a frame.</summary>
	protected virtual void Update()
	{
	}

	/// <summary>
	/// Called once a frame after animation has posed the bones and the engine has posed its own, as a First-Person
	/// View's camera, and before Objs on bones follow them: to bend bones, reach hands somewhere or place what a
	/// character holds. It comes after every behaviour's Update and before LateUpdate.
	/// </summary>
	protected virtual void PoseUpdate()
	{
	}

	/// <summary>Called once a frame, after every behaviour's Update: for a camera that follows what Update moved.</summary>
	protected virtual void LateUpdate()
	{
	}

	/// <summary>Called when the behaviour is disabled, and when it is destroyed, before OnDestroy.</summary>
	protected virtual void OnDisable()
	{
	}

	/// <summary>Called once, last, at the end of the frame the behaviour is destroyed in.</summary>
	protected virtual void OnDestroy()
	{
	}

	/// <summary>
	/// Called when the Obj's collider first touches another, straight after the physics step, as in Unity. One of the
	/// two must have a Rigidbody. Disabled behaviours are told too, so they can enable themselves.
	/// </summary>
	protected virtual void OnCollisionEnter(Collision collision)
	{
	}

	/// <summary>Called when the Obj's collider stops touching another. A body asleep on another still touches it.</summary>
	protected virtual void OnCollisionExit(Collision collision)
	{
	}

	/// <summary>
	/// Called when a collider enters the Obj's trigger, or the Obj enters another's trigger: a checkpoint passed, or a
	/// door that opens as the player comes near. Triggers notice Objs with a Rigidbody, and characters.
	/// </summary>
	protected virtual void OnTriggerEnter(Collider other)
	{
	}

	/// <summary>Called when a collider leaves the Obj's trigger, or the Obj leaves another's trigger.</summary>
	protected virtual void OnTriggerExit(Collider other)
	{
	}

	/// <summary>Called when the Obj's Collider2D first touches another, as OnCollisionEnter does in 3D.</summary>
	protected virtual void OnCollisionEnter2D(Collision2D collision)
	{
	}

	/// <summary>Called when the Obj's Collider2D stops touching another.</summary>
	protected virtual void OnCollisionExit2D(Collision2D collision)
	{
	}

	/// <summary>Called when a Collider2D enters the Obj's 2D trigger, or the Obj enters another's: a coin collected.</summary>
	protected virtual void OnTriggerEnter2D(Collider2D other)
	{
	}

	/// <summary>Called when a Collider2D leaves the Obj's 2D trigger, or the Obj leaves another's.</summary>
	protected virtual void OnTriggerExit2D(Collider2D other)
	{
	}

	/// <summary>
	/// Called when an action of the Obj's Motion Matcher, or of one on an Obj under it, passes one of its marks: the
	/// moment a grenade leaves the hand, or a spell is cast. It comes before LateUpdate, the frame the action passes
	/// it. Compare the mark with one picked in the Inspector.
	/// </summary>
	protected virtual void OnMotionMark(MotionMark mark)
	{
	}

	internal void PassMark(MotionMark mark)
	{
		OnMotionMark(mark);
	}

	internal void Collide(uint kind, Collision collision)
	{
		var (point, normal) = (collision.Point, collision.Normal);
		var flat = new Collision2D(collision.Obj, new Vector2(point.X, point.Y), new Vector2(normal.X, normal.Y));
		switch (kind)
		{
			case PhysicsEventData.CollisionEnter: OnCollisionEnter(collision); break;
			case PhysicsEventData.CollisionExit: OnCollisionExit(collision); break;
			case PhysicsEventData.TriggerEnter: OnTriggerEnter(collision.Collider); break;
			case PhysicsEventData.TriggerExit: OnTriggerExit(collision.Collider); break;
			case PhysicsEventData.CollisionEnter2D: OnCollisionEnter2D(flat); break;
			case PhysicsEventData.CollisionExit2D: OnCollisionExit2D(flat); break;
			case PhysicsEventData.TriggerEnter2D: OnTriggerEnter2D(flat.Collider); break;
			case PhysicsEventData.TriggerExit2D: OnTriggerExit2D(flat.Collider); break;
		}
	}

	/// <summary>
	/// Draws the behaviour's gizmos with Draw, each frame, in the editor's Scene views: in edit mode, from the values
	/// the scene gives it, and while the game plays. They show while the Scene view's Gizmos button is on, and never
	/// in the Game view or a build. One callback in place of Unity's two: gizmo.IsSelected says whether to show more.
	/// <code>
	/// protected override void DrawGizmos(Gizmo gizmo)
	/// {
	///     Draw.WireSphere(Obj.WorldPosition, Range, gizmo.IsSelected ? Color.Yellow : Color.Grey);
	/// }
	/// </code>
	/// </summary>
	protected virtual void DrawGizmos(Gizmo gizmo)
	{
	}

	internal void RunGizmos(Gizmo gizmo)
	{
		DrawGizmos(gizmo);
	}

	/// <summary>Whether a behaviour type draws gizmos: it, or a class between it and Behaviour, overrides DrawGizmos.</summary>
	internal static bool DrawsGizmos(System.Type type)
	{
		const System.Reflection.BindingFlags Instance =
			System.Reflection.BindingFlags.Instance | System.Reflection.BindingFlags.NonPublic | System.Reflection.BindingFlags.Public;
		var method = type.GetMethod(nameof(DrawGizmos), Instance, [typeof(Gizmo)]);
		return method != null && method.DeclaringType != typeof(Behaviour);
	}

	internal void Attach(Obj obj, uint handle, int order, bool enabled = true)
	{
		this.obj = obj;
		this.handle = handle;
		this.enabled = enabled;
		Order = order;
	}

	/// <summary>Takes the place of the same behaviour from the scripts' last build: its Obj, handle and state.</summary>
	internal void TakeOver(Behaviour previous, int order)
	{
		obj = previous.obj;
		handle = previous.handle;
		enabled = previous.enabled;
		IsDestroyed = previous.IsDestroyed;
		PendingFields = previous.PendingFields;
		Order = order;
	}

	internal void ApplyFields()
	{
		if (PendingFields == null)
		{
			return;
		}

		var fields = PendingFields;
		PendingFields = null;
		BehaviourFields.Apply(this, fields);
	}

	internal void Detach()
	{
		obj = Obj.None;
		IsDestroyed = true;
	}

	internal void Dispatch(Phase phase)
	{
		switch (phase)
		{
			case Phase.Awake: Awake(); break;
			case Phase.OnEnable: OnEnable(); break;
			case Phase.Start: Start(); break;
			case Phase.FixedUpdate: FixedUpdate(); break;
			case Phase.Update: Update(); break;
			case Phase.PoseUpdate: PoseUpdate(); break;
			case Phase.LateUpdate: LateUpdate(); break;
			case Phase.OnDisable: OnDisable(); break;
			case Phase.OnDestroy: OnDestroy(); break;
		}
	}

	private void SetEnabled(bool value)
	{
		if (enabled == value || IsDestroyed)
		{
			return;
		}

		enabled = value;
		Native.SetEnabled(handle, value);
	}
}
