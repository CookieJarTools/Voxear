using Voxear.Scripting;

namespace Voxear
{
	/// <summary>
	/// The live instance of an IInstance behaviour, for example Instance&lt;GameManager&gt;.Current.
	/// The lookup is cached until an instance comes or goes, so reading it every frame costs a compare.
	/// </summary>
	public static class Instance<T> where T : Behaviour, IInstance
	{
		private static T? cached;
		private static int version = -1;

		public static T? Current
		{
			get
			{
				if (version != InstanceRegistry.Version)
				{
					cached = InstanceRegistry.Get(typeof(T)) as T;
					version = InstanceRegistry.Version;
				}
				return cached;
			}
		}

		public static bool Exists => Current != null;
	}
}
