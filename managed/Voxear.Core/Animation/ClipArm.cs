using System;
using System.Collections.Generic;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// One arm of a <see cref="ClipRig"/>: its bones by their numbers, and its hand as the clip poses it, in the frame's
/// model space, read as <see cref="RigArm"/> reads a hand in the world.
/// </summary>
public sealed class ClipArm
{
	private readonly ClipFrame frame;
	private readonly int side;

	internal ClipArm(ClipFrame frame, int side, RigReader read)
	{
		this.frame = frame;
		this.side = side;
		Clavicle = read.Number();
		UpperArm = read.Number();
		Forearm = read.Number();
		Hand = read.Number();
		var fingers = new IReadOnlyList<int>[5];
		for (var finger = 0; finger < fingers.Length; finger++)
		{
			fingers[finger] = read.Numbers();
		}
		Fingers = fingers;
	}

	public bool IsLeft => side == 0;

	/// <summary>The collarbone, or -1 for a rig without one.</summary>
	public int Clavicle { get; }

	public int UpperArm { get; }

	public int Forearm { get; }

	public int Hand { get; }

	/// <summary>Each finger's bones from the knuckle out, thumb first; a finger the rig lacks has none.</summary>
	public IReadOnlyList<IReadOnlyList<int>> Fingers { get; }

	/// <summary>The way its fingers point, toward its middle finger.</summary>
	public Vector3 Pointing => Read(NativePose.HandAxes, 0);

	/// <summary>The way its palm faces.</summary>
	public Vector3 PalmFacing => Read(NativePose.HandAxes, 3);

	/// <summary>The middle of its palm, on its skin.</summary>
	public Vector3 PalmCenter => Read(NativePose.Palm, 0);

	/// <summary>The middle of its knuckles, where its fingers start.</summary>
	public Vector3 Knuckles => Read(NativePose.Knuckles, 0);

	/// <summary>How far its fingers are bent.</summary>
	public FingerBends Bends
	{
		get
		{
			var bends = new float[FingerBends.Count];
			return Call(NativePose.FingerBends, bends) == FingerBends.Count ? new FingerBends(bends) : FingerBends.Straight;
		}
	}

	/// <summary>How far it reaches from the shoulder to the middle of the palm, held out straight, as posed in the frame.</summary>
	public float Length
	{
		get
		{
			Span<float> answer = stackalloc float[1];
			return Call(NativePose.ArmReach, answer) == 1 ? answer[0] : 0f;
		}
	}

	public override string ToString()
	{
		return $"the {(IsLeft ? "left" : "right")} arm of {frame.Clip}";
	}

	private int Call(int op, Span<float> answer)
	{
		Span<int> which = stackalloc int[] { side };
		return frame.Run(op, which, answer);
	}

	private Vector3 Read(int op, int at)
	{
		Span<float> answer = stackalloc float[8];
		return Call(op, answer) >= at + 3 ? new Vector3(answer[at], answer[at + 1], answer[at + 2]) : Vector3.Zero;
	}
}
