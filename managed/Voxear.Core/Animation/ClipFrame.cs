using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A skeleton as a clip poses it at one time, read rather than shown: the clip's own skeleton, or a model's that the
/// clip is made to fit, as <see cref="Skeleton.Pose"/> would pose it. Places and turns are in that skeleton's model
/// space, since a clip is nowhere in the world. For reading where a clip has things, as where its hands hold what it
/// carries, without posing anything by it. Bones are by their numbers in that skeleton.
/// <code>
/// var frame = ClipFrame.Of(reload, 0.4f);
/// var palm = frame?.Rig?.RightArm.PalmCenter;
/// </code>
/// </summary>
public sealed class ClipFrame
{
	private const int BoneFloats = 23;

	private readonly float[] bones;
	private ClipRig? rig;
	private bool rigRead;

	private ClipFrame(AnimationClip clip, Obj onto, float time, bool looping, bool rest, float[] bones)
	{
		Clip = clip;
		Onto = onto;
		Time = time;
		Looping = looping;
		IsRest = rest;
		this.bones = bones;
	}

	public AnimationClip Clip { get; }

	/// <summary>The Obj whose model the clip is made to fit, or None for the clip's own skeleton.</summary>
	public Obj Onto { get; }

	/// <summary>The time in the clip, in seconds.</summary>
	public float Time { get; }

	public bool Looping { get; }

	/// <summary>Whether this is the skeleton at rest, as no clip poses it.</summary>
	public bool IsRest { get; }

	public int BoneCount => bones.Length / BoneFloats;

	/// <summary>Its humanoid rig, or null when its bones are not named as a humanoid's.</summary>
	public ClipRig? Rig
	{
		get
		{
			if (!rigRead)
			{
				rig = ClipRig.Of(this);
				rigRead = true;
			}
			return rig;
		}
	}

	/// <summary>
	/// The clip's own skeleton as it poses it at a time, or null while the engine is still finding the clip, as it
	/// does the first time a clip is asked for.
	/// </summary>
	public static ClipFrame? Of(AnimationClip clip, float time, bool looping = false)
	{
		return Read(clip, Obj.None, time, looping, false);
	}

	/// <summary>The skeleton of a model as the clip, made to fit it, would pose it at a time, leaving the model as it is.</summary>
	public static ClipFrame? Of(AnimationClip clip, Skeleton onto, float time, bool looping = false)
	{
		return onto.Obj.IsValid ? Read(clip, onto.Obj, time, looping, false) : null;
	}

	/// <summary>The clip's own skeleton at rest, as it stands with no clip posing it.</summary>
	public static ClipFrame? AtRest(AnimationClip clip)
	{
		return Read(clip, Obj.None, 0f, false, true);
	}

	/// <summary>A bone's place, turn and scale in the model's space.</summary>
	public Matrix4x4 Matrix(int bone)
	{
		if (bone < 0 || bone >= BoneCount)
		{
			return Matrix4x4.Identity;
		}
		var m = bones.AsSpan(bone * BoneFloats, 16);
		return new Matrix4x4(m[0], m[1], m[2], m[3], m[4], m[5], m[6], m[7], m[8], m[9], m[10], m[11], m[12], m[13], m[14], m[15]);
	}

	/// <summary>A bone's place in the model's space.</summary>
	public Vector3 Position(int bone)
	{
		return Matrix(bone).Translation;
	}

	/// <summary>A bone's turn in the model's space.</summary>
	public Quaternion Rotation(int bone)
	{
		return Matrix4x4.Decompose(Matrix(bone), out _, out var turn, out _) ? turn : Quaternion.Identity;
	}

	/// <summary>A bone's turn in its parent's space, as the clip sets it.</summary>
	public Quaternion LocalRotation(int bone)
	{
		var at = bone * BoneFloats + 16;
		return Within(bone) ? new Quaternion(bones[at], bones[at + 1], bones[at + 2], bones[at + 3]) : Quaternion.Identity;
	}

	/// <summary>A bone's place in its parent's space, as the clip sets it.</summary>
	public Vector3 LocalPosition(int bone)
	{
		var at = bone * BoneFloats + 20;
		return Within(bone) ? new Vector3(bones[at], bones[at + 1], bones[at + 2]) : Vector3.Zero;
	}

	/// <summary>The number of the bone with this name, or -1 when the skeleton has none.</summary>
	public int Find(string name)
	{
		Span<float> answer = stackalloc float[1];
		return Run(NativePose.FindBone, default, $"{Clip.Reference}\n{name}", answer) == 1 ? (int)answer[0] : -1;
	}

	internal int Run(int op, ReadOnlySpan<int> ints, Span<float> answer)
	{
		return Run(op, ints, Clip.Reference, answer);
	}

	private static ClipFrame? Read(AnimationClip clip, Obj onto, float time, bool looping, bool rest)
	{
		if (!clip.IsValid)
		{
			return null;
		}
		// Asked with room for one number, the engine says how many bones there are.
		var given = Given(time, looping, rest);
		Span<float> count = stackalloc float[1];
		if (NativePose.Run(onto, NativePose.ClipFrame | NativePose.OnClip, default, given, clip.Reference, count) != 1)
		{
			return null;
		}
		var read = new float[1 + (int)count[0] * BoneFloats];
		var written = NativePose.Run(onto, NativePose.ClipFrame | NativePose.OnClip, default, given, clip.Reference, read);
		return written == read.Length ? new ClipFrame(clip, onto, time, looping, rest, read[1..]) : null;
	}

	private static float[] Given(float time, bool looping, bool rest)
	{
		return [time, looping ? 1f : 0f, rest ? 1f : 0f];
	}

	private int Run(int op, ReadOnlySpan<int> ints, string text, Span<float> answer)
	{
		return NativePose.Run(Onto, op | NativePose.OnClip, ints, Given(Time, Looping, IsRest), text, answer);
	}

	private bool Within(int bone)
	{
		return bone >= 0 && bone < BoneCount;
	}
}
