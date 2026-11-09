using System;

namespace Voxear;

/// <summary>
/// An animation clip, by the model file that holds it and its name there, as <c>Models/Walks.glb#clip:Walk</c>. A
/// skeleton can be posed by any clip of its own rig, or of another humanoid's (see <see cref="Skeleton.Pose"/>). The
/// engine finds a clip the first time it is asked for, so for a frame or so it has no length yet.
/// </summary>
public readonly struct AnimationClip : IEquatable<AnimationClip>
{
	public AnimationClip(string reference)
	{
		Reference = reference;
	}

	/// <summary>The clip's file and name, as scenes keep it.</summary>
	public string Reference { get; }

	/// <summary>False for no clip.</summary>
	public bool IsValid => !string.IsNullOrEmpty(Reference);

	public static bool operator ==(AnimationClip left, AnimationClip right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(AnimationClip left, AnimationClip right)
	{
		return !left.Equals(right);
	}

	public bool Equals(AnimationClip other)
	{
		return Reference == other.Reference;
	}

	public override bool Equals(object? other)
	{
		return other is AnimationClip clip && Equals(clip);
	}

	public override int GetHashCode()
	{
		return Reference?.GetHashCode() ?? 0;
	}

	public override string ToString()
	{
		return Reference ?? "no clip";
	}
}
