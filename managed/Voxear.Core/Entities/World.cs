namespace Voxear
{
	/// <summary>
	/// The engine's entities, for code outside systems, such as a behaviour that counts what a system moves. Inside a
	/// system, its own Query is the one to use, since its Changed filters know when the system last ran.
	/// </summary>
	public static class World
	{
		public static EntityQuery Query<T1>()
			where T1 : unmanaged, IComponent
		{
			return new EntityQuery(0).Fetch<T1>();
		}

		public static EntityQuery Query<T1, T2>()
			where T1 : unmanaged, IComponent
			where T2 : unmanaged, IComponent
		{
			return new EntityQuery(0).Fetch<T1>().Fetch<T2>();
		}

		public static EntityQuery Query<T1, T2, T3>()
			where T1 : unmanaged, IComponent
			where T2 : unmanaged, IComponent
			where T3 : unmanaged, IComponent
		{
			return new EntityQuery(0).Fetch<T1>().Fetch<T2>().Fetch<T3>();
		}

		public static EntityQuery Query<T1, T2, T3, T4>()
			where T1 : unmanaged, IComponent
			where T2 : unmanaged, IComponent
			where T3 : unmanaged, IComponent
			where T4 : unmanaged, IComponent
		{
			return new EntityQuery(0).Fetch<T1>().Fetch<T2>().Fetch<T3>().Fetch<T4>();
		}
	}
}
