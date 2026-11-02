using System;
using System.Collections.Generic;

namespace Voxear;

/// <summary>
/// Names assets that code loads by path, so the content build ships them. The build reads levels, scenes,
/// prefabs and fields, but it cannot see a path put together in code, such as
/// <c>Prefab.Load($"Prefabs/Bosses/{boss}.prefab")</c>. Name the paths here, or patterns where * matches within
/// a folder and ** across folders. Like a field naming an asset, a content root ships even when
/// <c>[content] exclude</c> matches it, and the build warns about a pattern that matches nothing.
/// </summary>
/// <example>
/// <code>
/// [ContentRoot("Prefabs/Bosses/**")]
/// public sealed class BossSpawner : Behaviour
/// </code>
/// </example>
[AttributeUsage(
	AttributeTargets.Assembly | AttributeTargets.Class | AttributeTargets.Struct | AttributeTargets.Method |
	AttributeTargets.Field | AttributeTargets.Property,
	AllowMultiple = true)]
public sealed class ContentRootAttribute : Attribute
{
	public ContentRootAttribute(params string[] patterns)
	{
		Patterns = patterns;
	}

	/// <summary>Project paths, GUIDs or path patterns.</summary>
	public IReadOnlyList<string> Patterns { get; }
}
