using System;
using System.Text;
using Voxear.Scripting;

namespace Voxear.Tests;

internal static class GuideCheckTests
{
	[Test]
	private static void ChecksPassFailOrSayWhatIsStillWrong()
	{
		var assemblies = new[] { typeof(TestChecks).Assembly };
		const string type = "Voxear.Tests.TestChecks";
		Check.True(GuideChecks.Run(type, "Done", assemblies, out var passed, out _) && passed, "true passes");
		Check.True(GuideChecks.Run(type, "NotYet", assemblies, out passed, out var failure) && !passed && failure.Length == 0, "false fails, saying nothing");
		Check.True(GuideChecks.Run(type, "Fine", assemblies, out passed, out _) && passed, "no text passes");
		Check.True(GuideChecks.Run(type, "Missing", assemblies, out passed, out failure) && !passed && failure == "The Sun is missing", failure);
		Check.True(!GuideChecks.Run(type, "Count", assemblies, out _, out _), "a method returning anything else is no check");
		Check.True(!GuideChecks.Run(type, "Nothing", assemblies, out _, out _), "nor is one that is not there");
	}

	[Test]
	private static void WhatIsStillWrongIsCutAtACharactersEdge()
	{
		var buffer = new byte[4];
		var length = GuideChecks.Write("ab€c", buffer);
		Check.True(length == 2 && Encoding.UTF8.GetString(buffer, 0, length) == "ab", "the euro sign takes three bytes, so it goes whole");
		Check.True(GuideChecks.Write("abc", buffer) == 3, "what fits is written whole");
	}
}
