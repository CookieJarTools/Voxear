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

	/// <summary>The Obj's Collider, its shape for physics. Not valid when it has none.</summary>
	public Collider Collider => new Collider(this);

	/// <summary>Gives the Obj a Collider of a shape, unless it has one, and returns it.</summary>
	public Collider AddCollider(ColliderShape shape = ColliderShape.Box)
	{
		NativePhysics.Set(this, NativePhysics.ColliderAdd, new Vector4((int)shape, 0f, 0f, 0f));
		return new Collider(this);
	}

	/// <summary>The Obj's Rigidbody, which hands it to physics. Not valid when it has none.</summary>
	public Rigidbody Rigidbody => new Rigidbody(this);

	/// <summary>Gives the Obj a Rigidbody, unless it has one, and returns it. It needs a Collider too, for a shape.</summary>
	public Rigidbody AddRigidbody()
	{
		NativePhysics.Set(this, NativePhysics.BodyAdd, Vector4.Zero);
		return new Rigidbody(this);
	}

	/// <summary>The Obj's Buoyancy, which floats it on the water. Not valid when it has none.</summary>
	public Buoyancy Buoyancy => new Buoyancy(this);

	/// <summary>Gives the Obj a Buoyancy, unless it has one, and returns it. It needs a Rigidbody and a Collider too.</summary>
	public Buoyancy AddBuoyancy()
	{
		NativePhysics.Set(this, NativePhysics.BuoyancyAdd, Vector4.Zero);
		return new Buoyancy(this);
	}

	/// <summary>The Obj's Light. Not valid when it has none.</summary>
	public Light Light => new Light(this);

	/// <summary>Gives the Obj a Light of a kind, with that kind's usual settings, unless it has one, and returns it.</summary>
	public Light AddLight(LightKind kind = LightKind.Point)
	{
		NativeLights.Set(this, NativeLights.Add, new Vector4((int)kind, 0f, 0f, 0f));
		return new Light(this);
	}

	/// <summary>The Obj's Particle System. Not valid when it has none.</summary>
	public ParticleSystem Particles => new ParticleSystem(this);

	/// <summary>Gives the Obj a Particle System with the usual settings, playing, unless it has one, and returns it.</summary>
	public ParticleSystem AddParticles()
	{
		NativeParticles.Set(this, NativeParticles.Add, Vector4.Zero);
		return new ParticleSystem(this);
	}

	/// <summary>The Obj's Smoke. Not valid when it has none.</summary>
	public Smoke Smoke => new Smoke(this);

	/// <summary>Gives the Obj a Smoke with the usual settings, unless it has one, and returns it. It starts next frame; turn PlayOnStart off first to start it later with Play.</summary>
	public Smoke AddSmoke()
	{
		NativeSmoke.Set(this, NativeSmoke.Add, Vector4.Zero);
		return new Smoke(this);
	}

	/// <summary>The Obj's Mist. Not valid when it has none.</summary>
	public Mist Mist => new Mist(this);

	/// <summary>Gives the Obj a Mist with the usual settings, emitting, unless it has one, and returns it.</summary>
	public Mist AddMist()
	{
		NativeMist.Set(this, NativeMist.Add, Vector4.Zero);
		return new Mist(this);
	}

	/// <summary>The Obj's Motion Matcher, which animates its model. Not valid when it has none.</summary>
	public MotionMatcher MotionMatcher => new MotionMatcher(this);

	/// <summary>Gives the Obj a Motion Matcher with the usual settings, unless it has one, and returns it.</summary>
	public MotionMatcher AddMotionMatcher()
	{
		NativeMotionMatcher.Set(this, NativeMotionMatcher.Add, Vector4.Zero);
		return new MotionMatcher(this);
	}

	/// <summary>The Obj's Bone Attachment, keeping it on a bone of its parent's model. Not valid without one.</summary>
	public BoneAttachment BoneAttachment => new BoneAttachment(this);

	/// <summary>Gives the Obj a Bone Attachment, unless it has one, and returns it; its bone is set in the Inspector.</summary>
	public BoneAttachment AddBoneAttachment()
	{
		NativeBoneAttachment.Set(this, NativeBoneAttachment.Add, Vector4.Zero);
		return new BoneAttachment(this);
	}

	/// <summary>The Obj's Character Controller, which walks it. Not valid when it has none.</summary>
	public CharacterController CharacterController => new CharacterController(this);

	/// <summary>Gives the Obj a Character Controller, unless it has one, and returns it.</summary>
	public CharacterController AddCharacterController()
	{
		NativePhysics.Set(this, NativePhysics.CharacterAdd, Vector4.Zero);
		return new CharacterController(this);
	}

	/// <summary>
	/// The Obj's layer, by number from 0 to 31, as Unity's GameObject.layer: what physics keeps apart and asks after.
	/// LayerMask.NameToLayer finds one by the name the project gives it.
	/// </summary>
	public int Layer
	{
		get { return NativeLayers.Of(this); }
		set { NativeLayers.Set(this, value); }
	}

	/// <summary>
	/// Whether the Obj is drawn. False hides its meshes and models, and those of every Obj under it, as a wizard
	/// turned to mist is; its lights, effects, colliders and scripts carry on.
	/// </summary>
	public bool Visible
	{
		get { return NativeLayers.Visible(this); }
		set { NativeLayers.SetVisible(this, value); }
	}

	/// <summary>The Obj's Joint, which holds its Rigidbody to another or to the world. Not valid when it has none.</summary>
	public Joint Joint => new Joint(this);

	/// <summary>Gives the Obj a Joint of a type, unless it has one, held to a Rigidbody, or to the world for none.</summary>
	public Joint AddJoint(JointType type = JointType.Fixed, Rigidbody connectedBody = default)
	{
		NativePhysics.Set(this, NativePhysics.JointAdd, new Vector4((int)type, 0f, 0f, 0f));
		var joint = new Joint(this);
		if (connectedBody.Obj.IsValid)
		{
			joint.ConnectedBody = connectedBody;
		}
		return joint;
	}

	/// <summary>The Obj's Collider2D, its flat shape for 2D physics. Not valid when it has none.</summary>
	public Collider2D Collider2D => new Collider2D(this);

	/// <summary>Gives the Obj a Collider2D of a shape, unless it has one, and returns it.</summary>
	public Collider2D AddCollider2D(ColliderShape2D shape = ColliderShape2D.Box)
	{
		NativePhysics.Set(this, NativePhysics.Collider2DAdd, new Vector4((int)shape, 0f, 0f, 0f));
		return new Collider2D(this);
	}

	/// <summary>The Obj's Rigidbody2D, which hands it to 2D physics. Not valid when it has none.</summary>
	public Rigidbody2D Rigidbody2D => new Rigidbody2D(this);

	/// <summary>Gives the Obj a Rigidbody2D, unless it has one, and returns it. It needs a Collider2D too, for a shape.</summary>
	public Rigidbody2D AddRigidbody2D()
	{
		NativePhysics.Set(this, NativePhysics.Body2DAdd, Vector4.Zero);
		return new Rigidbody2D(this);
	}

	/// <summary>The Obj's Vehicle, which drives its Rigidbody on its children's Wheels. Not valid when it has none.</summary>
	public Vehicle Vehicle => new Vehicle(this);

	/// <summary>Gives the Obj a Vehicle of a kind, unless it has one, and returns it. It needs a Collider and a Rigidbody too.</summary>
	public Vehicle AddVehicle(VehicleKind kind = VehicleKind.Car)
	{
		NativePhysics.Set(this, NativePhysics.VehicleAdd, new Vector4((int)kind, 0f, 0f, 0f));
		return new Vehicle(this);
	}

	/// <summary>The Obj's Wheel, as one of its parent's Vehicle's wheels. Not valid when it has none.</summary>
	public Wheel Wheel => new Wheel(this);

	/// <summary>Gives the Obj a Wheel, unless it has one, and returns it. Its parent's Vehicle drives on it.</summary>
	public Wheel AddWheel()
	{
		NativePhysics.Set(this, NativePhysics.WheelAdd, Vector4.Zero);
		return new Wheel(this);
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
