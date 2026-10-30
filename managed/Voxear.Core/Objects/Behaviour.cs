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

		protected virtual void Awake()
		{
		}

		protected virtual void OnEnable()
		{
		}

		protected virtual void Start()
		{
		}

		protected virtual void FixedUpdate()
		{
		}

		protected virtual void Update()
		{
		}

		protected virtual void LateUpdate()
		{
		}

		protected virtual void OnDisable()
		{
		}

		protected virtual void OnDestroy()
		{
		}

		internal void Attach(Obj obj, uint handle, int order)
		{
			this.obj = obj;
			this.handle = handle;
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
