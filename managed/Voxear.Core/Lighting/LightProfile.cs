using System;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An IES light profile of the project, a .ies file from a lamp's maker: how bright a real fitting shines in each
/// direction, as a field holds it, picked in the Inspector. A point or spot Light shines by one as its Profile.
/// <code>
/// public LightProfile StreetLamp;
///
/// Obj.Light.Profile = StreetLamp;
/// </code>
/// </summary>
public readonly struct LightProfile : IEquatable<LightProfile>, IAsset<LightProfile>
{
	public static readonly LightProfile None = default;

	private readonly string? reference;

	private LightProfile(string reference)
	{
		this.reference = reference;
	}

	/// <summary>Whether it names a profile.</summary>
	public bool IsSet => !string.IsNullOrEmpty(reference);

	/// <summary>The profile's GUID or project path, as the scene or code gave it.</summary>
	public string Reference => reference ?? string.Empty;

	string? IAsset<LightProfile>.AssetGuid => IsSet ? NativeSaves.Key(NativeSaves.KeyReference, 0, Reference) : null;

	/// <summary>The profile at a project path or GUID. Nothing is read here, so this never fails.</summary>
	public static LightProfile Load(string path)
	{
		return new LightProfile(path);
	}

	public static bool operator ==(LightProfile left, LightProfile right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(LightProfile left, LightProfile right)
	{
		return !left.Equals(right);
	}

	public bool Equals(LightProfile other)
	{
		return Reference == other.Reference;
	}

	public override bool Equals(object? other)
	{
		return other is LightProfile graph && Equals(graph);
	}

	public override int GetHashCode()
	{
		return Reference.GetHashCode();
	}

	public override string ToString()
	{
		return IsSet ? Reference : "LightProfile(none)";
	}
}
