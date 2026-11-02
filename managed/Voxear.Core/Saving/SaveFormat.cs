using System;
using Voxear.Saving;

namespace Voxear;

/// <summary>
/// How a save's values become bytes, and back. Binary is small and quick, and is what builds write; JSON can be read
/// and changed in a text editor, and is what the editor and vox run write. A save records its format's Name, so it
/// is always read back with the format that wrote it, whatever Save.Format says now. To add a format, such as one
/// that compresses or encrypts another, derive from this class: the game finds it by itself.
/// </summary>
public abstract class SaveFormat
{
	/// <summary>Compact bytes, with each name and text written once however often it is used.</summary>
	public static SaveFormat Binary { get; } = new BinarySaveFormat();

	/// <summary>Indented JSON, which a person can read and change.</summary>
	public static SaveFormat Json { get; } = new JsonSaveFormat();

	/// <summary>What a save records to name its format, such as "binary". Keep it the same for good once saves use it.</summary>
	public abstract string Name { get; }

	public abstract byte[] Write(SaveNode root);

	/// <summary>Reads what Write wrote. Throw FormatException for bytes that make no sense, and the save reads as damaged.</summary>
	public abstract SaveNode Read(ReadOnlySpan<byte> bytes);

	public override string ToString()
	{
		return Name;
	}
}
