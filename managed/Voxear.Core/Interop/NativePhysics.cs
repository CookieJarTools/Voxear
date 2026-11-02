using System;
using System.Numerics;

namespace Voxear.Interop;

/// <summary>
/// The engine's physics, for Collider, Rigidbody, CharacterController and Physics: what each native call takes and
/// gives, as C# has it. Without the engine, as in tests, nothing has physics, and every value reads as missing.
/// </summary>
internal static unsafe class NativePhysics
{
	public const int ColliderHas = 0;
	public const int ColliderAdd = 1;
	public const int ColliderRemove = 2;
	public const int ColliderShape = 3;
	public const int ColliderCenter = 4;
	public const int ColliderSize = 5;
	public const int ColliderRadius = 6;
	public const int ColliderHeight = 7;
	public const int ColliderTrigger = 8;
	public const int ColliderFriction = 9;
	public const int ColliderBounciness = 10;

	public const int BodyHas = 20;
	public const int BodyAdd = 21;
	public const int BodyRemove = 22;
	public const int BodyMass = 23;
	public const int BodyKinematic = 24;
	public const int BodyGravity = 25;
	public const int BodyDrag = 26;
	public const int BodyAngularDrag = 27;
	public const int BodyContinuous = 28;
	public const int BodyVelocity = 29;
	public const int BodyAngularVelocity = 30;
	public const int BodyAwake = 31;

	public const int CharacterHas = 40;
	public const int CharacterAdd = 41;
	public const int CharacterRemove = 42;
	public const int CharacterHeight = 43;
	public const int CharacterRadius = 44;
	public const int CharacterSlopeLimit = 45;
	public const int CharacterStepOffset = 46;
	public const int CharacterGrounded = 47;
	public const int CharacterVelocity = 48;
	public const int CharacterGroundNormal = 49;

	public const int Gravity = 60;

	public const int PushForce = 0;
	public const int PushAcceleration = 1;
	public const int PushImpulse = 2;
	public const int PushVelocityChange = 3;
	public const int PushTorque = 4;
	public const int PushAngularImpulse = 5;

	public const int QueryTriggers = 1;

	/// <summary>A value of an Obj's physics, or false when the Obj has no such component.</summary>
	public static bool Get(Obj obj, int what, out Vector4 value)
	{
		var read = default(Vector4);
		var found = Native.IsConnected && Native.Api->PhysicsValue(obj.Id, what, 0, (float*)&read) == 1;
		value = read;
		return found;
	}

	/// <summary>Writes a value of an Obj's physics: false when the Obj has no such component.</summary>
	public static bool Set(Obj obj, int what, Vector4 value)
	{
		return Native.IsConnected && Native.Api->PhysicsValue(obj.Id, what, 1, (float*)&value) == 1;
	}

	public static float Number(Obj obj, int what, float missing)
	{
		return Get(obj, what, out var value) ? value.X : missing;
	}

	public static bool Flag(Obj obj, int what)
	{
		return Get(obj, what, out var value) && value.X != 0f;
	}

	public static Vector3 Vector(Obj obj, int what, Vector3 missing)
	{
		return Get(obj, what, out var value) ? new Vector3(value.X, value.Y, value.Z) : missing;
	}

	public static bool Push(Obj obj, int kind, Vector3 amount)
	{
		return Native.IsConnected && Native.Api->PhysicsPush(obj.Id, kind, (float*)&amount, null) == 1;
	}

	public static bool PushAt(Obj obj, int kind, Vector3 amount, Vector3 at)
	{
		return Native.IsConnected && Native.Api->PhysicsPush(obj.Id, kind, (float*)&amount, (float*)&at) == 1;
	}

	public static bool Raycast(Vector3 origin, Vector3 direction, float distance, bool triggers, out RaycastHitData hit)
	{
		var found = default(RaycastHitData);
		var flags = triggers ? QueryTriggers : 0;
		var met = Native.IsConnected
			&& Native.Api->PhysicsRaycast((float*)&origin, (float*)&direction, distance, flags, &found) == 1;
		hit = found;
		return met;
	}

	/// <summary>The Objs whose colliders a sphere overlaps, each once.</summary>
	public static Obj[] Overlap(Vector3 center, float radius, bool triggers)
	{
		if (!Native.IsConnected)
		{
			return Array.Empty<Obj>();
		}

		var flags = triggers ? QueryTriggers : 0;
		Span<ulong> some = stackalloc ulong[32];
		int count;
		fixed (ulong* written = some)
		{
			count = Native.Api->PhysicsOverlap((float*)&center, radius, flags, written, some.Length);
		}
		var ids = count <= some.Length ? some[..count].ToArray() : new ulong[count];
		if (count > some.Length)
		{
			fixed (ulong* written = ids)
			{
				count = Math.Min(count, Native.Api->PhysicsOverlap((float*)&center, radius, flags, written, ids.Length));
			}
		}
		return Array.ConvertAll(ids[..count], id => new Obj(id));
	}

	public static bool Move(Obj obj, Vector3 motion, float delta, out CharacterMoveData state)
	{
		var moved = default(CharacterMoveData);
		var done = Native.IsConnected && Native.Api->CharacterMove(obj.Id, (float*)&motion, delta, &moved) == 1;
		state = moved;
		return done;
	}
}
