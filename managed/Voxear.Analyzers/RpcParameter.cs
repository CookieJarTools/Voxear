namespace Voxear.Analyzers;

/// <summary>An RPC's parameter: its name, its type fully qualified, how it is read, and its default as written.</summary>
internal sealed class RpcParameter
{
	public RpcParameter(string name, string type, string? kind, string? fallback)
	{
		Name = name;
		Type = type;
		Kind = kind;
		Default = fallback;
	}

	public string Name { get; }

	public string Type { get; }

	/// <summary>The NetReader method that reads it, "enum" for an enum, or null for a type that cannot be sent.</summary>
	public string? Kind { get; }

	public string? Default { get; }
}
