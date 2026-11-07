using Microsoft.CodeAnalysis;

namespace Voxear.Analyzers;

/// <summary>A [Replicated] property as the generator found it: where it is, what it holds, and whether it can be written.</summary>
internal sealed class ReplicatedProperty
{
	public ReplicatedProperty(string space, string typeName, string fullTypeName, string name, string type, string access, string? kind, bool partial, Location location)
	{
		Namespace = space;
		TypeName = typeName;
		FullTypeName = fullTypeName;
		Name = name;
		Type = type;
		Access = access;
		Kind = kind;
		Partial = partial;
		Location = location;
	}

	public string Namespace { get; }

	public string TypeName { get; }

	public string FullTypeName { get; }

	public string Name { get; }

	/// <summary>Its type, fully qualified.</summary>
	public string Type { get; }

	public string Access { get; }

	/// <summary>The NetReader method that reads it, "enum" for an enum, or null for a type that cannot be replicated.</summary>
	public string? Kind { get; }

	/// <summary>Whether it is a partial property of a partial class that is neither nested nor generic.</summary>
	public bool Partial { get; }

	public Location Location { get; }
}
