using System;
using System.Diagnostics.CodeAnalysis;
using System.Numerics;
using Voxear.Interop;
using Voxear.Scripting;

namespace Voxear
{
	/// <summary>
	/// A thing in a scene, like a GameObject. It is a handle to an engine entity rather than a class,
	/// so passing one around never allocates, and an Obj whose entity is gone simply stops being valid.
	/// It is called Obj rather than Object because Object collides with System.Object in every file
	/// that uses System.
	/// </summary>
	public readonly struct Obj : IEquatable<Obj>
	{
		public static readonly Obj None = default;

		internal Obj(ulong id)
		{
			Id = id;
		}

		/// <summary>False for None. The engine treats generation zero as the null entity.</summary>
		public bool IsValid => (Id >> 32) != 0;

		public string Name => IsValid ? Native.ObjName(Id) : string.Empty;

		public Transform Transform
		{
			get { return Native.GetTransform(Id); }
			set { Native.SetTransform(Id, value); }
		}

		public Vector3 Position
		{
			get { return Transform.Position; }
			set { Transform = Transform with { Position = value }; }
		}

		public Quaternion Rotation
		{
			get { return Transform.Rotation; }
			set { Transform = Transform with { Rotation = value }; }
		}

		internal ulong Id { get; }

		/// <summary>
		/// Destroys this Obj and everything under it at the end of the frame, after their behaviours' OnDisable and
		/// OnDestroy, like Destroy(gameObject). They stop rendering straight away.
		/// </summary>
		public void Destroy()
		{
			if (IsValid)
			{
				Native.DestroyObj(Id);
			}
		}

		/// <summary>Attaches a new behaviour and runs its Awake and OnEnable before returning, like AddComponent.</summary>
		public T AddBehaviour<T>() where T : Behaviour
		{
			var handle = Native.AddBehaviour(Id, typeof(T).FullName!);
			if (handle < 0)
			{
				throw new InvalidOperationException($"Could not add {typeof(T).Name} to {this}.");
			}

			Activation.Run(new[] { (uint)handle });
			return (T)BehaviourTable.Get((uint)handle)!;
		}

		/// <summary>The first behaviour on this Obj that is a T, or null. Like GetComponent, it finds subclasses too.</summary>
		public T? GetBehaviour<T>() where T : Behaviour
		{
			foreach (var handle in Native.Behaviours(Id))
			{
				if (BehaviourTable.Get(handle) is T found)
				{
					return found;
				}
			}
			return null;
		}

		public bool TryGetBehaviour<T>([NotNullWhen(true)] out T? behaviour) where T : Behaviour
		{
			behaviour = GetBehaviour<T>();
			return behaviour != null;
		}

		public static bool operator ==(Obj left, Obj right)
		{
			return left.Id == right.Id;
		}

		public static bool operator !=(Obj left, Obj right)
		{
			return left.Id != right.Id;
		}

		public bool Equals(Obj other)
		{
			return Id == other.Id;
		}

		public override bool Equals(object? other)
		{
			return other is Obj obj && Equals(obj);
		}

		public override int GetHashCode()
		{
			return Id.GetHashCode();
		}

		public override string ToString()
		{
			return IsValid ? $"Obj({Name})" : "Obj(None)";
		}
	}
}
