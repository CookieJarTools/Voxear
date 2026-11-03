using System.Collections.Generic;
using System.Diagnostics.CodeAnalysis;
using Microsoft.CodeAnalysis.Diagnostics;

namespace Voxear.Analyzers.Tests;

/// <summary>The MSBuild properties a build shows its generators, such as build_property.RootNamespace.</summary>
internal sealed class BuildProperties : AnalyzerConfigOptions
{
	private readonly Dictionary<string, string> values;

	public BuildProperties(Dictionary<string, string> values)
	{
		this.values = values;
	}

	public override bool TryGetValue(string key, [NotNullWhen(true)] out string? value)
	{
		return values.TryGetValue(key, out value);
	}
}
