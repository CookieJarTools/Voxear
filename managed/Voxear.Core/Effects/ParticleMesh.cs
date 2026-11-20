using System;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A mesh a Particle System draws each particle as, in place of a flat picture facing the camera: one every project
/// has, such as ParticleMesh.Chunk, or one of the project's models, with ParticleMesh.Of. None draws them flat.
/// <code>
/// debris.Mesh = ParticleMesh.Chunk;
/// leaves.Mesh = ParticleMesh.Of(LeafModel);
/// </code>
/// </summary>
public readonly struct ParticleMesh : IEquatable<ParticleMesh>
{
	private const string Builtin = "builtin:";

	public static readonly ParticleMesh None = default;

	/// <summary>The model's GUID, or a built-in mesh's name after "builtin:"; empty for none.</summary>
	private readonly string? key;

	private ParticleMesh(string key)
	{
		this.key = key;
	}

	/// <summary>A box: bricks, crates' splinters, blocks.</summary>
	public static ParticleMesh Cube => new(Builtin + "cube");

	/// <summary>A ball: shot, stones worn round, beads.</summary>
	public static ParticleMesh Ball => new(Builtin + "ball");

	/// <summary>A lump with flat faces, no two sides alike: rubble, clods, shrapnel.</summary>
	public static ParticleMesh Chunk => new(Builtin + "chunk");

	/// <summary>A short rod: shell cases, splinters, twigs.</summary>
	public static ParticleMesh Rod => new(Builtin + "rod");

	/// <summary>Whether it names a mesh.</summary>
	public bool IsSet => !string.IsNullOrEmpty(key);

	internal string Key => key ?? string.Empty;

	/// <summary>The first mesh of one of the project's models, with its material.</summary>
	public static ParticleMesh Of(ModelAsset model)
	{
		IAsset<ModelAsset> asset = model;
		return model.IsSet ? new ParticleMesh(asset.AssetGuid ?? model.Reference) : None;
	}

	internal static ParticleMesh FromKey(string? key)
	{
		return string.IsNullOrEmpty(key) ? None : new ParticleMesh(key);
	}

	public static bool operator ==(ParticleMesh left, ParticleMesh right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(ParticleMesh left, ParticleMesh right)
	{
		return !left.Equals(right);
	}

	public bool Equals(ParticleMesh other)
	{
		return Key == other.Key;
	}

	public override bool Equals(object? other)
	{
		return other is ParticleMesh mesh && Equals(mesh);
	}

	public override int GetHashCode()
	{
		return Key.GetHashCode();
	}

	public override string ToString()
	{
		return IsSet ? Key : "ParticleMesh(none)";
	}
}
