using Voxear.Interop;
using Voxear.Scripting;

namespace Voxear
{
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

		internal void Attach(Obj obj, uint handle, int order)
		{
			this.obj = obj;
			this.handle = handle;
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
}
