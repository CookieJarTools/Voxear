using System;
using System.Diagnostics.CodeAnalysis;
using System.Numerics;
using Voxear.Interop;
using Voxear.Scripting;

namespace Voxear;

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

	/// <summary>The first Obj with this name, or None, like GameObject.Find. It looks through every Obj, so keep what it finds.</summary>
	public static Obj Find(string name)
	{
		var found = Native.FindObjs(name);
		return found.Length > 0 ? new Obj(found[0]) : None;
	}

	/// <summary>Every Obj with this name, oldest first.</summary>
	public static Obj[] FindAll(string name)
	{
		return Array.ConvertAll(Native.FindObjs(name), id => new Obj(id));
	}

	/// <summary>Position, rotation and scale relative to the parent, or to the world for an Obj without one.</summary>
	public Transform Transform
	{
		get { return Native.GetTransform(Id); }
		set { Native.SetTransform(Id, value); }
	}

	/// <summary>Relative to the parent, like Unity's localPosition. WorldPosition is in world space.</summary>
	public Vector3 Position
	{
		get { return Transform.Position; }
		set { Transform = Transform with { Position = value }; }
	}

	/// <summary>Relative to the parent, like Unity's localRotation. WorldRotation is in world space.</summary>
	public Quaternion Rotation
	{
		get { return Transform.Rotation; }
		set { Transform = Transform with { Rotation = value }; }
	}

	/// <summary>Where the Obj is in the world, through all its parents, as of now. Like Unity's position.</summary>
	public Vector3 WorldPosition => Native.GetWorldTransform(Id).Position;

	/// <summary>The Obj's rotation in the world, through all its parents. Like Unity's rotation.</summary>
	public Quaternion WorldRotation => Native.GetWorldTransform(Id).Rotation;

	/// <summary>The engine entity the Obj is, for the ECS: the same handle, with its components.</summary>
	public Entity Entity => new Entity(Id);

	/// <summary>The direction the Obj faces in the world: its +Z, as Unity's transform.forward.</summary>
	public Vector3 Forward => WorldRotation.Forward;

	public Vector3 Right => WorldRotation.Right;

	public Vector3 Up => WorldRotation.Up;

	internal ulong Id { get; }

	/// <summary>Turns the Obj so it faces a point in the world, keeping its up towards Vector3.Up.</summary>
	public void LookAt(Vector3 target)
	{
		LookAt(target, Vector3.Up);
	}

	/// <summary>Turns the Obj so it faces a point in the world, keeping its up as close to up as it can.</summary>
	public void LookAt(Vector3 target, Vector3 up)
	{
		var world = Native.GetWorldTransform(Id);
		var direction = target - world.Position;
		if (direction.LengthSquared() < 1e-10f)
		{
			return;
		}

		// The parent's rotation is what is left of the world rotation once the local one is taken off.
		var parent = world.Rotation * Quaternion.Inverse(Rotation);
		Rotation = Quaternion.Normalize(Quaternion.Inverse(parent) * Quaternion.LookRotation(direction, up));
	}

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
		return GetBehaviour(typeof(T)) as T;
	}

	/// <summary>The first behaviour on this Obj that is a type, or implements an interface, or null.</summary>
	public Behaviour? GetBehaviour(Type type)
	{
		foreach (var handle in Native.Behaviours(Id))
		{
			if (BehaviourTable.Get(handle) is { } behaviour && type.IsInstanceOfType(behaviour))
			{
				return behaviour;
			}
		}
		return null;
	}

	public bool TryGetBehaviour<T>([NotNullWhen(true)] out T? behaviour) where T : Behaviour
	{
		behaviour = GetBehaviour<T>();
		return behaviour != null;
	}

	/// <summary>
	/// Gives the Obj a component, the data systems work on, so a behaviour can hand an Obj to a system:
	/// <c>Obj.Add(new Spin { Speed = 90f })</c>. Returns the Obj, so adds chain.
	/// </summary>
	public Obj Add<T>(T value) where T : unmanaged, IComponent
	{
		Entity.Add(value);
		return this;
	}

	/// <summary>Gives the Obj the component with every field zero.</summary>
	public Obj Add<T>() where T : unmanaged, IComponent
	{
		Entity.Add<T>();
		return this;
	}

	public bool Has<T>() where T : unmanaged, IComponent
	{
		return Entity.Has<T>();
	}

	/// <summary>A copy of the component's value. Throws when the Obj has none; TryGet does not.</summary>
	public T Get<T>() where T : unmanaged, IComponent
	{
		return Entity.Get<T>();
	}

	public bool TryGet<T>(out T value) where T : unmanaged, IComponent
	{
		return Entity.TryGet(out value);
	}

	/// <summary>Writes the component's value in place.</summary>
	public void Set<T>(T value) where T : unmanaged, IComponent
	{
		Entity.Set(value);
	}

	public Obj Remove<T>() where T : unmanaged, IComponent
	{
		Entity.Remove<T>();
		return this;
	}

	/// <summary>The Obj's Audio Source, which plays a sound from it. Not valid when it has none.</summary>
	public AudioSource AudioSource => new AudioSource(this);

	/// <summary>
	/// Gives the Obj an Audio Source, unless it has one, and returns it: in 3D, playing the sound given, or none
	/// until one is set. Call Play on it to hear it.
	/// </summary>
	public AudioSource AddAudioSource(Sound sound = default)
	{
		NativeAudio.SetSource(this, NativeAudio.SourceAdd, 1f);
		var source = new AudioSource(this);
		if (sound.IsValid)
		{
			source.Sound = sound;
		}
		return source;
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
