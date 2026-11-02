using System;

namespace Voxear;

/// <summary>
/// The version of a type's saved shape. Raise it when a change means saves written before need changing to read,
/// and give the type a [MigrateFrom] method for the version before. A type without it is version 1. A save
/// records the version of each marked type in it, and refuses to read one written by a newer version.
/// </summary>
[AttributeUsage(AttributeTargets.Class | AttributeTargets.Struct, Inherited = false)]
public sealed class SaveVersionAttribute : Attribute
{
	public SaveVersionAttribute(int version)
	{
		Version = version;
	}

	public int Version { get; }
}
