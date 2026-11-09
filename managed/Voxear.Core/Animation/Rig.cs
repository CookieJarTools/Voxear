using System;
using System.Collections.Generic;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A humanoid's bones by what they are: the hips, the spine, the neck and head, each arm with its hand and fingers,
/// and each leg, found by the names most rigs give them. What posing a character by hand needs: reaching a hand
/// somewhere, closing its fingers, or turning the upper body.
/// <code>
/// var rig = Obj.Skeleton.Rig;
/// rig?.RightArm.Reach(handle.WorldPosition, pole: Obj.WorldPosition + Vector3.UnitY);
/// </code>
/// </summary>
public sealed class Rig
{
	private Rig(Skeleton skeleton, ReadOnlySpan<float> layout)
	{
		Skeleton = skeleton;
		var read = new RigReader(skeleton, layout);
		Hips = read.Bone();
		Neck = read.Bone();
		Head = read.Bone();
		Spine = read.Bones();
		LeftArm = new RigArm(skeleton, 0, read);
		RightArm = new RigArm(skeleton, 1, read);
		if (read.Number() == 1)
		{
			LeftLeg = new RigLeg(read);
			RightLeg = new RigLeg(read);
		}
	}

	/// <summary>The skeleton the bones are of.</summary>
	public Skeleton Skeleton { get; }

	public Bone Hips { get; }

	/// <summary>The neck, or <see cref="Bone.None"/> for a rig without one.</summary>
	public Bone Neck { get; }

	public Bone Head { get; }

	/// <summary>The spine from above the hips up to the chest, in order.</summary>
	public IReadOnlyList<Bone> Spine { get; }

	/// <summary>The top of the spine, where the arms hang: the hips for a rig with no spine.</summary>
	public Bone Chest => Spine.Count > 0 ? Spine[^1] : Hips;

	public RigArm LeftArm { get; }

	public RigArm RightArm { get; }

	/// <summary>The left leg, or null for a rig with none, as a pair of first-person arms.</summary>
	public RigLeg? LeftLeg { get; }

	public RigLeg? RightLeg { get; }

	/// <summary>A skeleton's rig, or null when its bones are not named as a humanoid's.</summary>
	public static Rig? Of(Skeleton skeleton)
	{
		Span<float> layout = stackalloc float[256];
		var written = NativePose.Run(skeleton.Obj, NativePose.Rig, default, default, null, layout);
		return written > 0 ? new Rig(skeleton, layout[..written]) : null;
	}

	/// <summary>Turns the spine, from the bottom up, by a turn in the world shared among its bones.</summary>
	public void TurnSpine(Quaternion turn)
	{
		Skeleton.TurnSpine(Spine, turn);
	}
}
