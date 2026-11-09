using System;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A UI document of the project, a .vui file, as a field holds it: picked in the Inspector, or dragged there from the
/// Project panel. It names the document by its ID, so renaming or moving the file keeps the field pointing at it.
/// <code>
/// [SerializeField] private UiDocumentAsset pauseMenu;
/// ...
/// var pause = UiDocument.Load(pauseMenu);
/// </code>
/// </summary>
public readonly struct UiDocumentAsset : IEquatable<UiDocumentAsset>, IAsset<UiDocumentAsset>
{
	public static readonly UiDocumentAsset None = default;

	private readonly string? reference;

	private UiDocumentAsset(string reference)
	{
		this.reference = reference;
	}

	/// <summary>Whether it names a UI document.</summary>
	public bool IsSet => !string.IsNullOrEmpty(reference);

	/// <summary>The document's GUID or project path, as the scene or code gave it.</summary>
	public string Reference => reference ?? string.Empty;

	string? IAsset<UiDocumentAsset>.AssetGuid => IsSet ? NativeSaves.Key(NativeSaves.KeyReference, 0, Reference) : null;

	/// <summary>The document at a project path or GUID, as "UI/Hud.vui". Nothing is read here, so this never fails.</summary>
	public static UiDocumentAsset Load(string path)
	{
		return new UiDocumentAsset(path);
	}

	public static bool operator ==(UiDocumentAsset left, UiDocumentAsset right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(UiDocumentAsset left, UiDocumentAsset right)
	{
		return !left.Equals(right);
	}

	public bool Equals(UiDocumentAsset other)
	{
		return Reference == other.Reference;
	}

	public override bool Equals(object? other)
	{
		return other is UiDocumentAsset asset && Equals(asset);
	}

	public override int GetHashCode()
	{
		return Reference.GetHashCode();
	}

	public override string ToString()
	{
		return IsSet ? Reference : "no UI document";
	}
}
