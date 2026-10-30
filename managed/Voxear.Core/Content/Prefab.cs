using System;
using System.Numerics;
using Voxear.Interop;
using Voxear.Scripting;

namespace Voxear
{
	/// <summary>
	/// A loaded prefab: an Obj hierarchy to copy into the world any number of times. Load it once, for example in
	/// Awake, then instantiate it whenever a copy is needed. Every copy's behaviours have had Awake and OnEnable by
	/// the time Instantiate returns, and get Start at the end of the current phase, as in Unity.
	/// </summary>
	public readonly struct Prefab : IEquatable<Prefab>, IAsset<Prefab>
	{
		public static readonly Prefab None = default;

		/// <summary>The engine's id plus one, so that the default value is None.</summary>
		private readonly int id;

		private Prefab(int engineId)
		{
			id = engineId + 1;
		}

		public bool IsValid => id > 0;

		/// <summary>
		/// Loads a prefab by project path or GUID, for example "Prefabs/Enemy.prefab", with its meshes and
		/// materials. Loading one that is already loaded costs nothing. Logs an error and returns None when the
		/// prefab cannot be loaded.
		/// </summary>
		public static Prefab Load(string path)
		{
			var loaded = Native.LoadPrefab(path);
			return loaded < 0 ? None : new Prefab(loaded);
		}

		/// <summary>A copy placed where the prefab places its own root.</summary>
		public Obj Instantiate()
		{
			return Spawn(null, false, Obj.None);
		}

		/// <summary>A copy under a parent, keeping the root's position and rotation relative to it.</summary>
		public Obj Instantiate(Obj parent)
		{
			return Spawn(null, false, parent);
		}

		/// <summary>A copy at a position and rotation in the world. The root keeps the prefab's scale.</summary>
		public Obj Instantiate(Vector3 position, Quaternion rotation)
		{
			return Spawn(Placement(position, rotation), true, Obj.None);
		}

		/// <summary>A copy under a parent, at a position and rotation in the world, like Unity's Instantiate.</summary>
		public Obj Instantiate(Vector3 position, Quaternion rotation, Obj parent)
		{
			return Spawn(Placement(position, rotation), true, parent);
		}

		public static bool operator ==(Prefab left, Prefab right)
		{
			return left.id == right.id;
		}

		public static bool operator !=(Prefab left, Prefab right)
		{
			return left.id != right.id;
		}

		public bool Equals(Prefab other)
		{
			return id == other.id;
		}

		public override bool Equals(object? other)
		{
			return other is Prefab prefab && Equals(prefab);
		}

		public override int GetHashCode()
		{
			return id;
		}

		private static Transform Placement(Vector3 position, Quaternion rotation)
		{
			return Transform.Identity with { Position = position, Rotation = rotation };
		}

		private unsafe Obj Spawn(Transform? placement, bool worldSpace, Obj parent)
		{
			if (!IsValid)
			{
				Log.Error("Instantiate was called on a Prefab that is not loaded.");
				return Obj.None;
			}

			var value = placement.GetValueOrDefault();
			var root = Native.Instantiate(id - 1, placement.HasValue ? &value : null, worldSpace, parent, out var handles);
			Activation.Run(handles);
			return root;
		}
	}
}
