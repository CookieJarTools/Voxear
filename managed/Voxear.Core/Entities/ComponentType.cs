using System;
using Voxear.Interop;

namespace Voxear
{
	/// <summary>
	/// A component type's id in the world scripts run in, registered by full name the first time it is asked for. The
	/// id belongs to that world, and play mode makes a new one each time it starts, so it is kept with the world's id
	/// and registered again when that changes. A static per type costs one comparison on each use.
	/// </summary>
	internal static unsafe class ComponentType<T> where T : unmanaged, IComponent
	{
		private static ulong world;
		private static uint id;

		public static uint Id
		{
			get
			{
				var current = *Native.Api->WorldId;
				if (current != world || current == 0)
				{
					Register(current);
				}
				return id;
			}
		}

		public static string Name => typeof(T).FullName ?? typeof(T).Name;

		private static void Register(ulong current)
		{
			var name = System.Text.Encoding.UTF8.GetBytes(Name);
			int registered;
			fixed (byte* text = name)
			{
				registered = Native.Api->ComponentRegister(text, name.Length, sizeof(T), Alignment());
			}
			if (registered < 0)
			{
				throw new InvalidOperationException($"{Name} cannot be used as a component here; the Console says why.");
			}

			id = (uint)registered;
			world = current;
		}

		/// <summary>
		/// C# has no alignof, so the alignment is the largest power of two that divides the size, up to 16. That is
		/// never less than the struct needs, only sometimes more, which costs a little padding between columns.
		/// </summary>
		private static int Alignment()
		{
			var size = sizeof(T);
			return Math.Min(size & -size, 16);
		}
	}
}
