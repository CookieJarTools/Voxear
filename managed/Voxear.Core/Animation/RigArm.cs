using System;
using System.Collections.Generic;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// One arm of a <see cref="Rig"/>: its bones, and what a script does with its hand after animation, in PoseUpdate.
/// Places and ways are in the world. A hand is placed by its wrist or by the middle of its palm, its elbow bending
/// toward a pole, and its fingers close round boxes, stopping where they meet them.
/// </summary>
public sealed class RigArm
{
	private readonly int side;

	internal RigArm(Skeleton skeleton, int side, RigReader read)
	{
		Skeleton = skeleton;
		this.side = side;
		Clavicle = read.Bone();
		UpperArm = read.Bone();
		Forearm = read.Bone();
		Hand = read.Bone();
		var fingers = new IReadOnlyList<Bone>[5];
		for (var finger = 0; finger < fingers.Length; finger++)
		{
			fingers[finger] = read.Bones();
		}
		Fingers = fingers;
	}

	public Skeleton Skeleton { get; }

	public bool IsLeft => side == 0;

	/// <summary>The collarbone, or <see cref="Bone.None"/> for a rig without one.</summary>
	public Bone Clavicle { get; }

	public Bone UpperArm { get; }

	public Bone Forearm { get; }

	public Bone Hand { get; }

	/// <summary>Each finger's bones from the hand out, thumb first; empty for a finger the rig lacks.</summary>
	public IReadOnlyList<IReadOnlyList<Bone>> Fingers { get; }

	/// <summary>The way its fingers point, toward its middle finger.</summary>
	public Vector3 Pointing => Read(NativePose.HandAxes, 0);

	/// <summary>The way its palm faces.</summary>
	public Vector3 PalmFacing => Read(NativePose.HandAxes, 3);

	/// <summary>The middle of its palm, on its skin.</summary>
	public Vector3 PalmCenter => Read(NativePose.Palm, 0);

	/// <summary>How far a finger's skin is from its bones.</summary>
	public float FingerThickness
	{
		get
		{
			Span<float> answer = stackalloc float[7];
			return Call(NativePose.Palm, default, answer) == 7 ? answer[6] : 0f;
		}
	}

	/// <summary>The middle of its knuckles, where its fingers start.</summary>
	public Vector3 Knuckles => Read(NativePose.Knuckles, 0);

	/// <summary>How far its fingers are bent now.</summary>
	public FingerBends Bends
	{
		get
		{
			var bends = new float[FingerBends.Count];
			return Call(NativePose.FingerBends, default, bends) == FingerBends.Count ? new FingerBends(bends) : FingerBends.Straight;
		}
	}

	/// <summary>
	/// Puts the wrist at a place, the elbow bending toward a pole; the collarbone first takes a share of a long reach,
	/// from 0 to 1. Gives back how far short the hand fell, 0 when it reached.
	/// </summary>
	public float Reach(Vector3 wrist, Vector3 pole, float clavicle = 0f)
	{
		Span<float> given = stackalloc float[7];
		Put(given, 0, wrist);
		Put(given, 3, pole);
		given[6] = clavicle;
		return Short(NativePose.ReachHand, given);
	}

	/// <summary>Turns the hand so its fingers point along one way and its palm faces another.</summary>
	public void Aim(Vector3 fingers, Vector3 palm)
	{
		Span<float> given = stackalloc float[6];
		Put(given, 0, fingers);
		Put(given, 3, palm);
		Call(NativePose.AimHand, given, default);
	}

	/// <summary>
	/// Puts the middle of the palm at a place, the hand turned so its fingers point along one way and its palm faces
	/// another, the elbow bending toward a pole. Gives back how far short it fell.
	/// </summary>
	public float Seat(Vector3 palmAt, Vector3 fingers, Vector3 palm, Vector3 pole, float clavicle = 0f)
	{
		Span<float> given = stackalloc float[13];
		Put(given, 0, palmAt);
		Put(given, 3, fingers);
		Put(given, 6, palm);
		Put(given, 9, pole);
		given[12] = clavicle;
		return Short(NativePose.SeatHand, given);
	}

	/// <summary>
	/// Closes the fingers from straight toward <paramref name="most"/>, each joint stopping where it would go into one
	/// of the boxes, easing from how they were, <paramref name="was"/>, over <paramref name="delta"/> seconds. Gives back
	/// how they are now, to hand back as <paramref name="was"/> next frame.
	/// </summary>
	public FingerBends CloseFingers(FingerBends most, FingerBends was, float delta, IReadOnlyList<OrientedBox>? around = null)
	{
		var boxes = around?.Count ?? 0;
		var given = new float[1 + FingerBends.Count * 2 + boxes * 22];
		given[0] = delta;
		most.CopyTo(given.AsSpan(1));
		was.CopyTo(given.AsSpan(1 + FingerBends.Count));
		for (var i = 0; i < boxes; i++)
		{
			around![i].CopyTo(given.AsSpan(1 + FingerBends.Count * 2 + i * 22));
		}
		var shown = new float[FingerBends.Count];
		return Call(NativePose.CloseFingers, given, shown) == FingerBends.Count ? new FingerBends(shown) : was;
	}

	/// <summary>
	/// How far the hand must move to be clear of the boxes, staying on the side of them it is on: its body from the
	/// wrist to the knuckles, and its forearm just behind the wrist. Its fingers close round them instead.
	/// </summary>
	public Vector3 KeepOut(IReadOnlyList<OrientedBox> around, Vector3 side)
	{
		var given = new float[3 + around.Count * 22];
		Put(given, 0, side);
		for (var i = 0; i < around.Count; i++)
		{
			around[i].CopyTo(given.AsSpan(3 + i * 22));
		}
		Span<float> moved = stackalloc float[3];
		return Call(NativePose.KeepOut, given, moved) == 3 ? new Vector3(moved[0], moved[1], moved[2]) : Vector3.Zero;
	}

	public override string ToString()
	{
		return $"{Skeleton.Obj}'s {(IsLeft ? "left" : "right")} arm";
	}

	private int Call(int op, ReadOnlySpan<float> given, Span<float> answer)
	{
		Span<int> which = stackalloc int[] { side };
		return NativePose.Run(Skeleton.Obj, op, which, given, null, answer);
	}

	private float Short(int op, ReadOnlySpan<float> given)
	{
		Span<float> answer = stackalloc float[1];
		return Call(op, given, answer) == 1 ? answer[0] : 0f;
	}

	private Vector3 Read(int op, int at)
	{
		Span<float> answer = stackalloc float[8];
		return Call(op, default, answer) >= at + 3 ? new Vector3(answer[at], answer[at + 1], answer[at + 2]) : Vector3.Zero;
	}

	private static void Put(Span<float> to, int at, Vector3 value)
	{
		to[at] = value.X;
		to[at + 1] = value.Y;
		to[at + 2] = value.Z;
	}
}
