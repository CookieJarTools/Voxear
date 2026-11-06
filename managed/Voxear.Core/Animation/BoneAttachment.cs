using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Bone Attachment: it keeps the Obj on a bone of its parent's model as the model is posed, as eyes on a
/// character's head or a gun in its hand. The bone is chosen in the Inspector. Get it with Obj.BoneAttachment, or give
/// an Obj one with Obj.AddBoneAttachment.
/// <code>
/// // First person: eyes kept on the head, the head hidden from the camera there.
/// var attachment = eyes.BoneAttachment;
/// attachment.Hide = true;
/// </code>
/// </summary>
public readonly struct BoneAttachment : IEquatable<BoneAttachment>, IObjComponent
{
	public static readonly BoneAttachment None = default;

	internal BoneAttachment(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj it keeps on the bone.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Bone Attachment, or is gone.</summary>
	public bool IsValid => Obj.IsValid && NativeBoneAttachment.Number(Obj, NativeBoneAttachment.Has, 0f) != 0f;

	/// <summary>Where it sits from the bone, in metres: in the parent's space, or in the bone's when it turns with it.</summary>
	public Vector3 Offset
	{
		get { return NativeBoneAttachment.Get(Obj, NativeBoneAttachment.Offset, out var value) ? new Vector3(value.X, value.Y, value.Z) : Vector3.Zero; }
		set { Set(NativeBoneAttachment.Offset, new Vector4(value, 0f)); }
	}

	/// <summary>Whether it turns as the bone turns, rather than keeping its own turn.</summary>
	public bool Turn
	{
		get { return NativeBoneAttachment.Number(Obj, NativeBoneAttachment.Turn, 0f) != 0f; }
		set { Set(NativeBoneAttachment.Turn, new Vector4(value ? 1f : 0f, 0f, 0f, 0f)); }
	}

	/// <summary>Whether the bone, and every bone under it, shrinks to nothing, as a first-person camera in the head wants.</summary>
	public bool Hide
	{
		get { return NativeBoneAttachment.Number(Obj, NativeBoneAttachment.Hide, 0f) != 0f; }
		set { Set(NativeBoneAttachment.Hide, new Vector4(value ? 1f : 0f, 0f, 0f, 0f)); }
	}

	/// <summary>Takes the Bone Attachment off its Obj, which stays where it last was.</summary>
	public void Remove()
	{
		NativeBoneAttachment.Set(Obj, NativeBoneAttachment.Remove, Vector4.Zero);
	}

	public static bool operator ==(BoneAttachment left, BoneAttachment right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(BoneAttachment left, BoneAttachment right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(BoneAttachment other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is BoneAttachment attachment && Equals(attachment);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Bone Attachment";
	}

	private void Set(int what, Vector4 value)
	{
		if (!NativeBoneAttachment.Set(Obj, what, value) && Native.IsConnected)
		{
			Log.Error($"{Obj} has no Bone Attachment; give it one with AddBoneAttachment.");
		}
	}
}
