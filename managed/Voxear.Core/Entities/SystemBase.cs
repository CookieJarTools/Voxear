namespace Voxear
{
	/// <summary>
	/// Logic that runs over many entities at once, as Unity's SystemBase: the advanced path beside behaviours, for
	/// things there are hundreds or thousands of. The engine makes one of each system class in the game's scripts and
	/// runs its Update once a frame in its stage (see UpdateIn), in ExecutionOrder and then name order.
	/// <code>
	/// public sealed class SpinSystem : SystemBase
	/// {
	/// 	protected override void Update()
	/// 	{
	/// 		Query&lt;Transform, Spin&gt;().ForEach((ref Transform transform, ref Spin spin) =&gt;
	/// 			transform.Rotation *= Quaternion.CreateFromAxisAngle(Vector3.UnitY, spin.Speed * Time.Delta));
	/// 	}
	/// }
	/// </code>
	/// Adding and removing components and destroying entities wait until Update returns. A system is made again, with
	/// OnCreate, each time play mode starts and each time the scripts reload, so it keeps nothing it needs in between.
	/// </summary>
	public abstract class SystemBase
	{
		/// <summary>A disabled system is skipped until it is enabled again.</summary>
		public bool Enabled { get; set; } = true;

		/// <summary>The world's change tick when the system last finished, which its Changed filters look after.</summary>
		internal uint LastRun { get; set; }

		/// <summary>Its sample in the Profiler, made with the system.</summary>
		internal uint Marker { get; set; }

		protected EntityQuery Query<T1>()
			where T1 : unmanaged, IComponent
		{
			return new EntityQuery(LastRun).Fetch<T1>();
		}

		protected EntityQuery Query<T1, T2>()
			where T1 : unmanaged, IComponent
			where T2 : unmanaged, IComponent
		{
			return new EntityQuery(LastRun).Fetch<T1>().Fetch<T2>();
		}

		protected EntityQuery Query<T1, T2, T3>()
			where T1 : unmanaged, IComponent
			where T2 : unmanaged, IComponent
			where T3 : unmanaged, IComponent
		{
			return new EntityQuery(LastRun).Fetch<T1>().Fetch<T2>().Fetch<T3>();
		}

		protected EntityQuery Query<T1, T2, T3, T4>()
			where T1 : unmanaged, IComponent
			where T2 : unmanaged, IComponent
			where T3 : unmanaged, IComponent
			where T4 : unmanaged, IComponent
		{
			return new EntityQuery(LastRun).Fetch<T1>().Fetch<T2>().Fetch<T3>().Fetch<T4>();
		}

		/// <summary>Once, before the first Update.</summary>
		protected virtual void OnCreate()
		{
		}

		protected abstract void Update();

		/// <summary>Before the scripts reload. When play mode stops, the world goes, and this is not called.</summary>
		protected virtual void OnDestroy()
		{
		}

		internal void RunCreate()
		{
			OnCreate();
		}

		internal void RunUpdate()
		{
			Update();
		}

		internal void RunDestroy()
		{
			OnDestroy();
		}
	}
}
