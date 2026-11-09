using System;
using System.Collections.Generic;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A humanoid's bones in a <see cref="ClipFrame"/>, by their numbers in its skeleton, as <see cref="Rig"/> has them for
/// a model in the world: the hips, the spine, the neck and head, each arm with its hand and fingers, and each leg. A
/// bone the rig lacks is -1.
/// </summary>
public sealed class ClipRig
{
	private ClipRig(ClipFrame frame, ReadOnlySpan<float> layout)
	{
		Frame = frame;
		var read = new RigReader(default, layout);
		Hips = read.Number();
		Neck = read.Number();
		Head = read.Number();
		Spine = read.Numbers();
		LeftArm = new ClipArm(frame, 0, read);
		RightArm = new ClipArm(frame, 1, read);
		if (read.Number() == 1)
		{
			LeftLeg = new ClipLeg(read);
			RightLeg = new ClipLeg(read);
		}
	}

	/// <summary>The frame the bones are posed in.</summary>
	public ClipFrame Frame { get; }

	public int Hips { get; }

	public int Neck { get; }

	public int Head { get; }

	/// <summary>The spine from above the hips up to the chest, in order.</summary>
	public IReadOnlyList<int> Spine { get; }

	/// <summary>The top of the spine, where the arms hang: the hips for a rig with no spine.</summary>
	public int Chest => Spine.Count > 0 ? Spine[^1] : Hips;

	public ClipArm LeftArm { get; }

	public ClipArm RightArm { get; }

	public ClipLeg? LeftLeg { get; }

	public ClipLeg? RightLeg { get; }

	internal static ClipRig? Of(ClipFrame frame)
	{
		Span<float> layout = stackalloc float[256];
		var written = frame.Run(NativePose.Rig, default, layout);
		return written > 0 ? new ClipRig(frame, layout[..written]) : null;
	}
}
