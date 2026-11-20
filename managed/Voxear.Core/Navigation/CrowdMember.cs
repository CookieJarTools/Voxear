using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's own place in a crowd: a tier of its own, how much it matters, and whether it is kept as a record while far
/// off. Get it with Obj.CrowdMember, or give an Obj one with Obj.AddCrowdMember.
/// <code>
/// // The boss never stutters, however far the camera pulls back.
/// boss.AddCrowdMember().Tier = CrowdTier.Near;
///
/// // Villagers spawned for the town go as records when no one is near, and come back where they walked to.
/// villager.AddCrowdMember().KeepAsRecord = true;
/// </code>
/// </summary>
public readonly struct CrowdMember : IEquatable<CrowdMember>, IObjComponent
{
	private const int TierValue = 3;
	private const int ImportanceValue = 4;
	private const int KeepAsRecordValue = 5;
	private const int TierNowValue = 6;

	public static readonly CrowdMember None = default;

	internal CrowdMember(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj in the crowd.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Crowd Member, or is gone.</summary>
	public bool IsValid => Obj.IsValid && Number(NativeComponent.Has) != 0f;

	/// <summary>Its tier by its distance, or always this one.</summary>
	public CrowdTier Tier
	{
		get { return (CrowdTier)(int)Number(TierValue); }
		set { Write(TierValue, (int)value); }
	}

	/// <summary>How much it matters, which scales the distances its tier is found by: 2 stays near twice as far out.</summary>
	public float Importance
	{
		get { return Number(ImportanceValue); }
		set { Write(ImportanceValue, value); }
	}

	/// <summary>Whether it goes as a record beyond Project Settings ▸ Crowds ▸ Records beyond, its Obj gone until it comes near.</summary>
	public bool KeepAsRecord
	{
		get { return Number(KeepAsRecordValue) != 0f; }
		set { Write(KeepAsRecordValue, value ? 1f : 0f); }
	}

	/// <summary>The tier it is in this frame: Near, Middle or Far.</summary>
	public CrowdTier TierNow => (CrowdTier)(int)NativeComponent.Number(NativeComponent.CrowdMember, Obj, TierNowValue, 1f);

	/// <summary>Takes the Crowd Member off its Obj; its tier is by its distance again from the next frame.</summary>
	public void Remove()
	{
		NativeComponent.Set(NativeComponent.CrowdMember, Obj, NativeComponent.Remove, Vector4.Zero);
	}

	public static bool operator ==(CrowdMember left, CrowdMember right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(CrowdMember left, CrowdMember right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(CrowdMember other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is CrowdMember member && Equals(member);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Crowd Member";
	}

	private float Number(int what)
	{
		return NativeComponent.Number(NativeComponent.CrowdMember, Obj, what, 0f);
	}

	private void Write(int what, float value)
	{
		if (!NativeComponent.Set(NativeComponent.CrowdMember, Obj, what, new Vector4(value, 0f, 0f, 0f)) && Native.IsConnected)
		{
			Log.Error($"{Obj} has no Crowd Member; give it one with AddCrowdMember.");
		}
	}
}
