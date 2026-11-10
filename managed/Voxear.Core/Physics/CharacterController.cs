using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Character Controller, which walks it where a script says, as Unity's CharacterController: a capsule
/// standing on the Obj's position that slides along walls, climbs steps up to StepOffset and slopes up to SlopeLimit.
/// Get it with Obj.CharacterController, or give an Obj one with Obj.AddCharacterController.
/// <para>
/// Physics never moves a character on its own, so gravity is the script's to add: Move takes the whole motion, a
/// jump included, and SimpleMove adds the fall itself.
/// </para>
/// <para>
/// A character stands against the pull where it is: on a planet, away from its middle, so it walks all the way round.
/// Up says which way that is, and Physics.GravityAt which way it falls.
/// </para>
/// <code>
/// protected override void Update()
/// {
///     var walk = Input.Action("Move").Vector * 4f;
///     Obj.CharacterController.SimpleMove(new Vector3(walk.X, 0f, walk.Y));
/// }
/// </code>
/// </summary>
public readonly struct CharacterController : IEquatable<CharacterController>, IObjComponent
{
	public static readonly CharacterController None = default;

	internal CharacterController(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj it walks.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Character Controller, or is gone.</summary>
	public bool IsValid => Obj.IsValid && NativePhysics.Flag(Obj, NativePhysics.CharacterHas);

	/// <summary>Whether it is switched on, as Unity's enabled. Off, it neither walks nor collides.</summary>
	public bool Enabled
	{
		get { return Obj.IsEnabled(EngineComponent.CharacterController); }
		set { Obj.SetEnabled(EngineComponent.CharacterController, value); }
	}

	/// <summary>How tall it is, from its feet up, in metres.</summary>
	public float Height
	{
		get { return NativePhysics.Number(Obj, NativePhysics.CharacterHeight, 1.8f); }
		set { Set(NativePhysics.CharacterHeight, value); }
	}

	/// <summary>How wide it is, as the capsule's radius.</summary>
	public float Radius
	{
		get { return NativePhysics.Number(Obj, NativePhysics.CharacterRadius, 0.3f); }
		set { Set(NativePhysics.CharacterRadius, value); }
	}

	/// <summary>The steepest slope it walks up, in degrees; steeper, it slides down.</summary>
	public float SlopeLimit
	{
		get { return NativePhysics.Number(Obj, NativePhysics.CharacterSlopeLimit, 45f); }
		set { Set(NativePhysics.CharacterSlopeLimit, value); }
	}

	/// <summary>The highest step it climbs without jumping, in metres.</summary>
	public float StepOffset
	{
		get { return NativePhysics.Number(Obj, NativePhysics.CharacterStepOffset, 0.3f); }
		set { Set(NativePhysics.CharacterStepOffset, value); }
	}

	/// <summary>Whether it stood on ground it can walk on after its last move.</summary>
	public bool IsGrounded => NativePhysics.Flag(Obj, NativePhysics.CharacterGrounded);

	/// <summary>How fast it went on its last move, after walls and the ground stopped what they could.</summary>
	public Vector3 Velocity => NativePhysics.Vector(Obj, NativePhysics.CharacterVelocity, Vector3.Zero);

	/// <summary>Which way the ground under it faced after its last move: up on flat ground.</summary>
	public Vector3 GroundNormal => NativePhysics.Vector(Obj, NativePhysics.CharacterGroundNormal, Vector3.UnitY);

	/// <summary>
	/// Which way it stands, against the pull where it is: away from a planet's middle near one, and up elsewhere. Its
	/// capsule tilts to it, and the ground it stands on, the slopes it climbs and its steps are measured against it.
	/// </summary>
	public Vector3 Up => NativePhysics.Vector(Obj, NativePhysics.CharacterUp, Vector3.UnitY);

	/// <summary>
	/// Moves it by motion, in metres, at once, sliding along what is in the way and climbing steps: usually a velocity
	/// times Time.Delta. Gravity is not added, so a falling character's motion includes its fall.
	/// </summary>
	public void Move(Vector3 motion)
	{
		if (!Native.IsConnected || Time.Delta <= 0f)
		{
			return;
		}
		if (!NativePhysics.Move(Obj, motion, Time.Delta, out _))
		{
			Log.Error($"{Obj} has no Character Controller; give it one with AddCharacterController.");
		}
	}

	/// <summary>
	/// Walks it at speed, in metres per second, across the ground, falling when nothing holds it up, as Unity's
	/// SimpleMove: the speed along Up is left out, its Y on flat ground. On a planet it falls towards the planet's
	/// middle. Call it once a frame. Returns whether it is on the ground.
	/// </summary>
	public bool SimpleMove(Vector3 speed)
	{
		if (!Native.IsConnected)
		{
			return false;
		}

		var delta = Time.Delta;
		var up = Up;
		var falling = IsGrounded ? 0f : Vector3.Dot(Velocity, up);
		falling += Vector3.Dot(Physics.GravityAt(Obj.WorldPosition), up) * delta;
		var across = speed - Vector3.Dot(speed, up) * up;
		Move((across + up * falling) * delta);
		return IsGrounded;
	}

	/// <summary>Takes the Character Controller off its Obj.</summary>
	public void Remove()
	{
		NativePhysics.Set(Obj, NativePhysics.CharacterRemove, Vector4.Zero);
	}

	public static bool operator ==(CharacterController left, CharacterController right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(CharacterController left, CharacterController right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(CharacterController other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is CharacterController controller && Equals(controller);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Character Controller";
	}

	private void Set(int what, float value)
	{
		if (!NativePhysics.Set(Obj, what, new Vector4(value, 0f, 0f, 0f)) && Native.IsConnected)
		{
			Log.Error($"{Obj} has no Character Controller; give it one with AddCharacterController.");
		}
	}
}
