using System.Numerics;

namespace Voxear.Interop;

/// <summary>
/// The engine's Bone Attachments, for BoneAttachment: what each value's number is, as the engine's bone_attachment.rs
/// has them. Without the engine, as in tests, no Obj has one, and every value reads as missing.
/// </summary>
internal static unsafe class NativeBoneAttachment
{
	public const int Has = 0;
	public const int Add = 1;
	public const int Remove = 2;
	public const int Offset = 3;
	public const int Turn = 4;
	public const int Hide = 5;

	/// <summary>A value of an Obj's Bone Attachment, or false when the Obj has none.</summary>
	public static bool Get(Obj obj, int what, out Vector4 value)
	{
		var read = default(Vector4);
		var found = Native.IsConnected && Native.Api->BoneAttachmentValue(obj.Id, what, 0, (float*)&read) == 1;
		value = read;
		return found;
	}

	/// <summary>Writes a value of an Obj's Bone Attachment: false when the Obj has none.</summary>
	public static bool Set(Obj obj, int what, Vector4 value)
	{
		return Native.IsConnected && Native.Api->BoneAttachmentValue(obj.Id, what, 1, (float*)&value) == 1;
	}

	public static float Number(Obj obj, int what, float missing)
	{
		return Get(obj, what, out var value) ? value.X : missing;
	}
}
