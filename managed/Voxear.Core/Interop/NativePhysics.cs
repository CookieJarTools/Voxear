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
	public const int ColliderConvex = 11;
	public const int ColliderAcoustic = 12;

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

	public const int JointHas = 70;
	public const int JointAdd = 71;
	public const int JointRemove = 72;
	public const int JointType = 73;
	public const int JointAnchor = 74;
	public const int JointAxis = 75;
	public const int JointLimits = 76;

	public const int Collider2DHas = 100;
	public const int Collider2DAdd = 101;
	public const int Collider2DRemove = 102;
	public const int Collider2DShape = 103;
	public const int Collider2DOffset = 104;
	public const int Collider2DSize = 105;
	public const int Collider2DRadius = 106;
	public const int Collider2DHeight = 107;
	public const int Collider2DTrigger = 108;
	public const int Collider2DFriction = 109;
	public const int Collider2DBounciness = 110;

	public const int Body2DHas = 120;
	public const int Body2DAdd = 121;
	public const int Body2DRemove = 122;
	public const int Body2DMass = 123;
	public const int Body2DKinematic = 124;
	public const int Body2DGravityScale = 125;
	public const int Body2DDrag = 126;
	public const int Body2DAngularDrag = 127;
	public const int Body2DFixedRotation = 128;
	public const int Body2DContinuous = 129;
	public const int Body2DVelocity = 130;
	public const int Body2DAngularVelocity = 131;
	public const int Body2DAwake = 132;

	public const int VehicleHas = 200;
	public const int VehicleAdd = 201;
	public const int VehicleRemove = 202;
	public const int VehicleKind = 203;
	public const int VehicleMaxTorque = 204;
	public const int VehicleMinRpm = 205;
	public const int VehicleMaxRpm = 206;
	public const int VehicleAutomatic = 207;
	public const int VehicleThrottle = 208;
	public const int VehicleSteering = 209;
	public const int VehicleBrake = 210;
	public const int VehicleHandbrake = 211;
	public const int VehicleSpeed = 212;
	public const int VehicleRpm = 213;
	public const int VehicleGear = 214;
	public const int VehicleGears = 215;
	public const int VehicleFirstGear = 216;
	public const int VehicleTopGear = 217;
	public const int VehicleAntiRoll = 218;

	public const int WheelHas = 220;
	public const int WheelAdd = 221;
	public const int WheelRemove = 222;
	public const int WheelRadius = 223;
	public const int WheelWidth = 224;
	public const int WheelSuspension = 225;
	public const int WheelSpring = 226;
	public const int WheelDamping = 227;
	public const int WheelSteerAngle = 228;
	public const int WheelPowered = 229;
	public const int WheelBrake = 230;
	public const int WheelHandbrake = 231;
	public const int WheelGrip = 232;
	public const int WheelSideGrip = 233;

	public const int Gravity2D = 160;

	public const int PushForce = 0;
	public const int PushAcceleration = 1;
	public const int PushImpulse = 2;
	public const int PushVelocityChange = 3;
	public const int PushTorque = 4;
	public const int PushAngularImpulse = 5;
	public const int Push2DForce = 10;
	public const int Push2DImpulse = 11;
	public const int Push2DTorque = 12;
	public const int Push2DAngularImpulse = 13;

	public const int QueryTriggers = 1;
	public const int Query2D = 2;

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

	public static Vector2 Vector2(Obj obj, int what, Vector2 missing)
	{
		return Get(obj, what, out var value) ? new Vector2(value.X, value.Y) : missing;
	}

	public static bool Push(Obj obj, int kind, Vector3 amount)
	{
		return Native.IsConnected && Native.Api->PhysicsPush(obj.Id, kind, (float*)&amount, null) == 1;
	}

	public static bool PushAt(Obj obj, int kind, Vector3 amount, Vector3 at)
	{
		return Native.IsConnected && Native.Api->PhysicsPush(obj.Id, kind, (float*)&amount, (float*)&at) == 1;
	}

	public static bool Raycast(Vector3 origin, Vector3 direction, float distance, int mask, bool triggers, out RaycastHitData hit)
	{
		return Raycast(origin, direction, distance, mask, triggers ? QueryTriggers : 0, out hit);
	}

	public static bool Raycast2D(Vector2 origin, Vector2 direction, float distance, int mask, bool triggers, out RaycastHitData hit)
	{
		var flags = Query2D | (triggers ? QueryTriggers : 0);
		return Raycast(new Vector3(origin, 0f), new Vector3(direction, 0f), distance, mask, flags, out hit);
	}

	private static bool Raycast(Vector3 origin, Vector3 direction, float distance, int mask, int flags, out RaycastHitData hit)
	{
		var found = default(RaycastHitData);
		var met = Native.IsConnected
			&& Native.Api->PhysicsRaycast((float*)&origin, (float*)&direction, distance, (uint)mask, flags, &found) == 1;
		hit = found;
		return met;
	}

	/// <summary>The Objs on the layers in mask whose colliders a sphere overlaps, each once.</summary>
	public static Obj[] Overlap(Vector3 center, float radius, int mask, bool triggers)
	{
		return Overlap(center, radius, mask, triggers ? QueryTriggers : 0);
	}

	/// <summary>The Objs on the layers in mask whose 2D colliders a circle overlaps, each once.</summary>
	public static Obj[] Overlap2D(Vector2 center, float radius, int mask, bool triggers)
	{
		return Overlap(new Vector3(center, 0f), radius, mask, Query2D | (triggers ? QueryTriggers : 0));
	}

	private static Obj[] Overlap(Vector3 center, float radius, int mask, int flags)
	{
		if (!Native.IsConnected)
		{
			return Array.Empty<Obj>();
		}

		Span<ulong> some = stackalloc ulong[32];
		int count;
		fixed (ulong* written = some)
		{
			count = Native.Api->PhysicsOverlap((float*)&center, radius, (uint)mask, flags, written, some.Length);
		}
		var ids = count <= some.Length ? some[..count].ToArray() : new ulong[count];
		if (count > some.Length)
		{
			fixed (ulong* written = ids)
			{
				var again = Native.Api->PhysicsOverlap((float*)&center, radius, (uint)mask, flags, written, ids.Length);
				count = Math.Min(count, again);
			}
		}
		return Array.ConvertAll(ids[..count], id => new Obj(id));
	}

	/// <summary>The Obj an Obj's joint holds to, or None for the world.</summary>
	public static Obj Connected(Obj obj)
	{
		return Native.IsConnected ? new Obj(Native.Api->JointConnected(obj.Id, 0, 0)) : Obj.None;
	}

	public static bool Connect(Obj obj, Obj other)
	{
		return Native.IsConnected && Native.Api->JointConnected(obj.Id, 1, other.Id) == 1;
	}

	public static bool Move(Obj obj, Vector3 motion, float delta, out CharacterMoveData state)
	{
		var moved = default(CharacterMoveData);
		var done = Native.IsConnected && Native.Api->CharacterMove(obj.Id, (float*)&motion, delta, &moved) == 1;
		state = moved;
		return done;
	}
}
