using System.Collections.Generic;
using Microsoft.CodeAnalysis;

namespace Voxear.Analyzers;

/// <summary>A [ServerRpc] or [ClientRpc] method as the generator found it: where it is, what it takes, and how it is sent.</summary>
internal sealed class RpcMethod
{
	public RpcMethod(string space, string typeName, string fullTypeName, string name, string access, bool server, Location location)
	{
		Namespace = space;
		TypeName = typeName;
		FullTypeName = fullTypeName;
		Name = name;
		Access = access;
		Server = server;
		Location = location;
	}

	public string Namespace { get; }

	public string TypeName { get; }

	public string FullTypeName { get; }

	public string Name { get; }

	public string Access { get; }

	/// <summary>A [ServerRpc], rather than a [ClientRpc].</summary>
	public bool Server { get; }

	public Location Location { get; }

	public List<RpcParameter> Parameters { get; } = [];

	/// <summary>Whether Voxear can write its sender: a void method, without type or ref parameters, of a partial class neither nested nor generic.</summary>
	public bool Writable { get; set; }

	public bool AnyPlayer { get; set; }

	public bool Unreliable { get; set; }

	public int PerSecond { get; set; }

	public string Attribute
	{
		get { return Server ? "ServerRpc" : "ClientRpc"; }
	}

	/// <summary>Its key, the same on every machine: a hash of its type's full name and its own.</summary>
	public uint Key
	{
		get
		{
			var hash = 2166136261u;
			foreach (var c in $"{FullTypeName}.{Name}")
			{
				hash = (hash ^ c) * 16777619u;
			}
			return hash;
		}
	}
}
