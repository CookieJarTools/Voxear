using System;
using System.Collections.Generic;
using System.Linq;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The bones of the model an Obj draws, posed as animation and scripts have them this frame. Read a bone anywhere;
/// set one in PoseUpdate, after animation has posed it, and what is set shows this frame:
/// <code>
/// protected override void PoseUpdate()
/// {
/// 	var head = Obj.Skeleton.Find("Head");
/// 	head.Rotation = Quaternion.CreateFromAxisAngle(Vector3.UnitY, 0.3f) * head.Rotation;
/// }
/// </code>
/// </summary>
public readonly struct Skeleton : IEquatable<Skeleton>
{
	public static readonly Skeleton None = default;

	internal Skeleton(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj that draws the model.</summary>
	public Obj Obj { get; }

	/// <summary>False when the Obj draws no model with bones, or is gone.</summary>
	public bool IsValid => BoneCount > 0;

	/// <summary>How many bones the model has.</summary>
	public int BoneCount => Obj.IsValid ? (int)NativeComponent.Number(NativeComponent.Skeleton, Obj, Bone.CountValue, 0f) : 0;

	/// <summary>A bone by its number, parents before their children; <see cref="Bone.None"/> past the last.</summary>
	public Bone this[int index] => index >= 0 && index < BoneCount ? new Bone(Obj, index) : Bone.None;

	/// <summary>Every bone, parents before their children.</summary>
	public IEnumerable<Bone> Bones
	{
		get
		{
			var obj = Obj;
			return Enumerable.Range(0, BoneCount).Select(index => new Bone(obj, index));
		}
	}

	/// <summary>The bone with this name, matched exactly or else whatever its case; <see cref="Bone.None"/> when none is.</summary>
	public Bone Find(string name)
	{
		var answer = NativeComponent.Ask(NativeComponent.Skeleton, Obj, Bone.FindText, name);
		return int.TryParse(answer, out var index) && index >= 0 ? new Bone(Obj, index) : Bone.None;
	}

	/// <summary>
	/// The model's humanoid rig, its bones by what they are, or null when they are not named as a humanoid's. It is
	/// found each time it is asked for, so keep it.
	/// </summary>
	public Rig? Rig => Voxear.Rig.Of(this);

	/// <summary>
	/// Bends a limb of three bones, as a leg or a tail, so its end reaches a place in the world, the middle joint
	/// bending toward a pole. Gives back how far short it fell, 0 when it reached.
	/// </summary>
	public float TwoBone(Bone top, Bone middle, Bone end, Vector3 target, Vector3 pole)
	{
		Span<int> bones = stackalloc int[] { top.Index, middle.Index, end.Index };
		Span<float> given = stackalloc float[] { target.X, target.Y, target.Z, pole.X, pole.Y, pole.Z };
		Span<float> answer = stackalloc float[1];
		return NativePose.Run(Obj, NativePose.TwoBone, bones, given, null, answer) == 1 ? answer[0] : 0f;
	}

	/// <summary>Turns a run of bones, from the bottom up, by a turn in the world shared among them, as a spine bends.</summary>
	public void TurnSpine(IReadOnlyList<Bone> spine, Quaternion turn)
	{
		var bones = spine.Select(bone => bone.Index).ToArray();
		Span<float> given = stackalloc float[] { turn.X, turn.Y, turn.Z, turn.W };
		NativePose.Run(Obj, NativePose.TurnSpine, bones, given, null, default);
	}

	/// <summary>
	/// Poses the model by a clip at a time in seconds, <paramref name="weight"/> of the way from how it is posed now:
	/// every bone, or only those given. A clip of another humanoid's rig is fitted to this one. Gives back false while
	/// the engine is still finding the clip, as it is the first frame it is asked for, or when it cannot be found.
	/// </summary>
	public bool Pose(AnimationClip clip, float time, float weight = 1f, bool looping = false, IReadOnlyList<Bone>? only = null)
	{
		var bones = only?.Select(bone => bone.Index).ToArray() ?? Array.Empty<int>();
		Span<float> given = stackalloc float[] { time, weight, looping ? 1f : 0f };
		Span<float> answer = stackalloc float[1];
		return NativePose.Run(Obj, NativePose.SampleClip, bones, given, clip.Reference, answer) == 1;
	}

	/// <summary>How long a clip lasts, in seconds, or null while the engine is still finding it.</summary>
	public float? ClipLength(AnimationClip clip)
	{
		Span<float> answer = stackalloc float[1];
		return NativePose.Run(Obj, NativePose.ClipLength, default, default, clip.Reference, answer) == 1 ? answer[0] : null;
	}

	/// <summary>
	/// A box round what each of the model's skinned bones moves, where the bone is now, in the world: the surfaces of
	/// a skinned model a hand closes on. Bones in <paramref name="leaving"/> are left out, as a bolt in the hand.
	/// </summary>
	public IReadOnlyList<OrientedBox> SkinBoxes(IReadOnlyCollection<Bone>? leaving = null)
	{
		var answer = new float[1 + BoneCount * 7];
		var written = NativePose.Run(Obj, NativePose.BoneBoxes, default, default, null, answer);
		var boxes = new List<OrientedBox>();
		for (var at = 1; written > 0 && at + 7 <= written; at += 7)
		{
			var bone = this[(int)answer[at]];
			if (!bone.IsValid || (leaving?.Contains(bone) ?? false))
			{
				continue;
			}
			var frame = Matrix4x4.CreateFromQuaternion(bone.Rotation) * Matrix4x4.CreateTranslation(bone.Position);
			var least = new Vector3(answer[at + 1], answer[at + 2], answer[at + 3]);
			var most = new Vector3(answer[at + 4], answer[at + 5], answer[at + 6]);
			boxes.Add(new OrientedBox(frame, least, most));
		}
		return boxes;
	}

	public static bool operator ==(Skeleton left, Skeleton right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(Skeleton left, Skeleton right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(Skeleton other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is Skeleton skeleton && Equals(skeleton);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Skeleton";
	}
}
