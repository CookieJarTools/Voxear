using System;
using System.Text;

namespace Voxear.Interop;

/// <summary>
/// What scripts do to a posed model in one call each, numbered as the engine's animation/pose_ops.rs numbers them:
/// bones and sides go in as whole numbers, places, ways and weights as floats, a clip by its reference.
/// </summary>
internal static unsafe class NativePose
{
	public const int Rig = 0;
	public const int TwoBone = 1;
	public const int ReachHand = 2;
	public const int AimHand = 3;
	public const int HandAxes = 4;
	public const int Palm = 5;
	public const int SeatHand = 6;
	public const int CloseFingers = 7;
	public const int FingerBends = 8;
	public const int KeepOut = 9;
	public const int Knuckles = 10;
	public const int TurnSpine = 11;
	public const int SampleClip = 12;
	public const int ClipLength = 13;
	public const int BoneBoxes = 14;

	/// <summary>Does an op on the model an Obj draws: how many floats it wrote into output, or -1 when it could not.</summary>
	public static int Run(Obj model, int op, ReadOnlySpan<int> ints, ReadOnlySpan<float> floats, string? text, Span<float> output)
	{
		if (!Native.IsConnected || !model.IsValid)
		{
			return -1;
		}
		var bytes = text == null ? Array.Empty<byte>() : Encoding.UTF8.GetBytes(text);
		fixed (int* givenInts = ints)
		fixed (float* givenFloats = floats)
		fixed (byte* givenText = bytes)
		fixed (float* answer = output)
		{
			return Native.Api->PoseOp(model.Id, op, givenInts, ints.Length, givenFloats, floats.Length, givenText, bytes.Length, answer, output.Length);
		}
	}
}
