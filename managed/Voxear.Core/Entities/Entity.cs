using System;
using Voxear.Interop;

namespace Voxear
{
	/// <summary>
	/// A handle to an entity in the engine's world: an Obj, or plain data with no name or place, as systems make for
	/// things there are too many of to be Objs. Its components are unmanaged structs (see IComponent). A handle whose
	/// entity is gone stops being alive, and is never mistaken for a newer entity in the same slot.
	/// <para>
	/// Adding or removing a component and destroying an entity move it between the engine's chunks, so while a system
	/// runs, or a query is being walked, they wait and apply once it is done. Reading and writing a value in place
	/// happens at once.
	/// </para>
	/// </summary>
	public readonly unsafe struct Entity : IEquatable<Entity>
	{
		public static readonly Entity Null = default;

		internal Entity(ulong bits)
		{
			Bits = bits;
		}

		/// <summary>Whether the entity still exists. Null never does.</summary>
		public bool IsAlive => Bits >> 32 != 0 && Native.Api->EntityAlive(Bits) != 0;

		/// <summary>The Obj this entity is. Only meaningful for an entity that is one.</summary>
		public Obj Obj => new Obj(Bits);

		internal ulong Bits { get; }

		/// <summary>A new entity with no components, made at once, even inside a system.</summary>
		public static Entity Create()
		{
			return new Entity(Native.Api->EntityCreate());
		}

		/// <summary>Destroys the entity. An Obj is destroyed as Obj.Destroy does, with its children and behaviours.</summary>
		public void Destroy()
		{
			Native.Api->EntityDestroy(Bits);
		}

		public bool Has<T>() where T : unmanaged, IComponent
		{
			return Native.Api->ComponentHas(Bits, ComponentType<T>.Id) != 0;
		}

		/// <summary>A copy of the component's value. Throws when the entity has none; TryGet does not.</summary>
		public T Get<T>() where T : unmanaged, IComponent
		{
			if (!TryGet(out T value))
			{
				throw new InvalidOperationException($"{this} has no {typeof(T).Name}");
			}
			return value;
		}

		public bool TryGet<T>(out T value) where T : unmanaged, IComponent
		{
			T read = default;
			var status = Native.Api->ComponentGet(Bits, ComponentType<T>.Id, (byte*)&read, sizeof(T));
			value = read;
			return status > 0;
		}

		/// <summary>Writes the component's value in place. Throws when the entity has none, since Add is what gives it one.</summary>
		public void Set<T>(T value) where T : unmanaged, IComponent
		{
			var status = Native.Api->ComponentSet(Bits, ComponentType<T>.Id, (byte*)&value, sizeof(T));
			if (status == 0)
			{
				throw new InvalidOperationException($"{this} has no {typeof(T).Name}; Add gives it one");
			}
		}

		/// <summary>Gives the entity the component, or a new value for the one it has. Returns the entity, so adds chain.</summary>
		public Entity Add<T>(T value) where T : unmanaged, IComponent
		{
			Native.Api->ComponentAdd(Bits, ComponentType<T>.Id, (byte*)&value, sizeof(T));
			return this;
		}

		/// <summary>Gives the entity the component with every field zero.</summary>
		public Entity Add<T>() where T : unmanaged, IComponent
		{
			return Add(default(T));
		}

		public Entity Remove<T>() where T : unmanaged, IComponent
		{
			Native.Api->ComponentRemove(Bits, ComponentType<T>.Id);
			return this;
		}

		public static bool operator ==(Entity left, Entity right)
		{
			return left.Bits == right.Bits;
		}

		public static bool operator !=(Entity left, Entity right)
		{
			return left.Bits != right.Bits;
		}

		public bool Equals(Entity other)
		{
			return Bits == other.Bits;
		}

		public override bool Equals(object? other)
		{
			return other is Entity entity && Equals(entity);
		}

		public override int GetHashCode()
		{
			return Bits.GetHashCode();
		}

		/// <summary>Its slot and generation, as Entity(12v3).</summary>
		public override string ToString()
		{
			return $"Entity({(uint)Bits}v{Bits >> 32})";
		}
	}
}
