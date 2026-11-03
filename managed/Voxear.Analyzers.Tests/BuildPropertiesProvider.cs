using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.Diagnostics;

namespace Voxear.Analyzers.Tests;

/// <summary>Gives generators a build's properties, and nothing for any one file.</summary>
internal sealed class BuildPropertiesProvider : AnalyzerConfigOptionsProvider
{
	private static readonly BuildProperties None = new([]);

	public BuildPropertiesProvider(BuildProperties global)
	{
		GlobalOptions = global;
	}

	public override AnalyzerConfigOptions GlobalOptions { get; }

	public override AnalyzerConfigOptions GetOptions(SyntaxTree tree)
	{
		return None;
	}

	public override AnalyzerConfigOptions GetOptions(AdditionalText textFile)
	{
		return None;
	}
}
