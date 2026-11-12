using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Block Mob: it walks the Obj through a Block World by itself. Tell it where to go and it finds a way over
/// the blocks, round walls, up steps a block high and down drops, walks it with the Obj's Character Controller, and
/// turns to face the way it goes. In water too deep to stand in it swims. With a Mob Motion on the same Obj, the model
/// walks as it does. Get it with Obj.BlockMob, or give an Obj one with Obj.AddBlockMob.
/// <code>
/// // A pet that keeps near the player, and wanders once it is close.
/// var mob = Obj.BlockMob;
/// if (Vector3.Distance(player.Position, Obj.Position) > 6f)
/// {
/// 	mob.Follow(player, 2f);
/// }
/// else if (mob.Arrived)
/// {
/// 	mob.Wander(4f);
/// }
/// </code>
/// </summary>
public readonly struct BlockMob : IEquatable<BlockMob>, IObjComponent
{
	public static readonly BlockMob None = default;

	internal BlockMob(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj it walks.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Block Mob, or is gone.</summary>
	public bool IsValid => Obj.IsValid && NativeBlockMob.Number(Obj, NativeBlockMob.Has, 0f) != 0f;

	/// <summary>Whether it is where it was told to go, or has nowhere to go. Wandering, it is true between walks.</summary>
	public bool Arrived => NativeBlockMob.Number(Obj, NativeBlockMob.Arrived, 1f) != 0f;

	/// <summary>What it is doing about where it is.</summary>
	public MobGoal Goal => (MobGoal)(int)NativeBlockMob.Number(Obj, NativeBlockMob.Goal, 0f);

	/// <summary>How fast it walks, in metres a second.</summary>
	public float Speed
	{
		get { return NativeBlockMob.Number(Obj, NativeBlockMob.Speed, 0f); }
		set { Set(NativeBlockMob.Speed, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>How fast it turns to face the way it goes, in degrees a second.</summary>
	public float TurnSpeed
	{
		get { return NativeBlockMob.Number(Obj, NativeBlockMob.TurnSpeed, 0f); }
		set { Set(NativeBlockMob.TurnSpeed, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>Whether it swims up in water too deep to stand in, or sinks.</summary>
	public bool Swims
	{
		get { return NativeBlockMob.Number(Obj, NativeBlockMob.Swims, 0f) != 0f; }
		set { Set(NativeBlockMob.Swims, new Vector4(value ? 1f : 0f, 0f, 0f, 0f)); }
	}

	/// <summary>Walks to a point in the world, by whatever way over the blocks there is.</summary>
	public void MoveTo(Vector3 point)
	{
		Set(NativeBlockMob.MoveTo, new Vector4(point, 0f));
	}

	/// <summary>Walks after an Obj wherever it goes, stopping within near metres of it.</summary>
	public void Follow(Obj obj, float near = 2f)
	{
		// The Obj's number in two halves, each carried in a float's bits.
		var low = BitConverter.Int32BitsToSingle(unchecked((int)(obj.Id & 0xFFFFFFFF)));
		var high = BitConverter.Int32BitsToSingle(unchecked((int)(obj.Id >> 32)));
		Set(NativeBlockMob.Follow, new Vector4(low, high, near, 0f));
	}

	/// <summary>Goes here and there, no further than radius metres from where it is now, standing a while between.</summary>
	public void Wander(float radius)
	{
		Set(NativeBlockMob.Wander, new Vector4(radius, 0f, 0f, 0f));
	}

	/// <summary>Stands where it is.</summary>
	public void Stop()
	{
		Set(NativeBlockMob.Stop, Vector4.Zero);
	}

	/// <summary>Takes the Block Mob off its Obj, which stays where it is.</summary>
	public void Remove()
	{
		NativeBlockMob.Set(Obj, NativeBlockMob.Remove, Vector4.Zero);
	}

	public static bool operator ==(BlockMob left, BlockMob right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(BlockMob left, BlockMob right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(BlockMob other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is BlockMob mob && Equals(mob);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Block Mob";
	}

	private void Set(int what, Vector4 value)
	{
		if (!NativeBlockMob.Set(Obj, what, value) && Native.IsConnected)
		{
			Log.Error($"{Obj} has no Block Mob; give it one with AddBlockMob.");
		}
	}
}
