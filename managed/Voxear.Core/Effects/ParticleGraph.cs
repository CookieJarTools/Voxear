using System;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A particle graph of the project: a VoxShade graph that moves and colours a Particle System's particles in place of
/// its own settings, as a field holds it, picked in the Inspector. It names the graph by its ID, so renaming or
/// moving the file keeps the field pointing at it.
/// <code>
/// public ParticleGraph Swirl;
///
/// Obj.Particles.Graph = Swirl;
/// </code>
/// </summary>
public readonly struct ParticleGraph : IEquatable<ParticleGraph>, IAsset<ParticleGraph>
{
	public static readonly ParticleGraph None = default;

	private readonly string? reference;

	private ParticleGraph(string reference)
	{
		this.reference = reference;
	}

	/// <summary>Whether it names a graph.</summary>
	public bool IsSet => !string.IsNullOrEmpty(reference);

	/// <summary>The graph's GUID or project path, as the scene or code gave it.</summary>
	public string Reference => reference ?? string.Empty;

	string? IAsset<ParticleGraph>.AssetGuid => IsSet ? NativeSaves.Key(NativeSaves.KeyReference, 0, Reference) : null;

	/// <summary>The graph at a project path or GUID. Nothing is read here, so this never fails.</summary>
	public static ParticleGraph Load(string path)
	{
		return new ParticleGraph(path);
	}

	public static bool operator ==(ParticleGraph left, ParticleGraph right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(ParticleGraph left, ParticleGraph right)
	{
		return !left.Equals(right);
	}

	public bool Equals(ParticleGraph other)
	{
		return Reference == other.Reference;
	}

	public override bool Equals(object? other)
	{
		return other is ParticleGraph graph && Equals(graph);
	}

	public override int GetHashCode()
	{
		return Reference.GetHashCode();
	}

	public override string ToString()
	{
		return IsSet ? Reference : "ParticleGraph(none)";
	}
}
