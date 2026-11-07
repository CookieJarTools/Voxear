using System;
using System.Linq;
using System.Reflection;

namespace Voxear.Analyzers.Tests;

/// <summary>Runs every static method whose name starts with "Test" in the test classes, and exits non-zero when any fails.</summary>
internal static class Program
{
	private static int Main()
	{
		var tests = new[] { typeof(AnalyzerTests), typeof(ModuleTests), typeof(RpcTests), typeof(StringKeysTests) }
			.SelectMany(type => type.GetMethods(BindingFlags.Static | BindingFlags.NonPublic | BindingFlags.Public)
				.Where(method => method.Name.StartsWith("Test", StringComparison.Ordinal))
				.OrderBy(method => method.Name))
			.ToList();
		var failed = tests.Count(method => !Run(method));
		Console.WriteLine($"{tests.Count - failed} passed, {failed} failed");
		return failed == 0 ? 0 : 1;
	}

	private static bool Run(MethodInfo test)
	{
		try
		{
			test.Invoke(null, null);
			Console.WriteLine($"ok    {test.Name}");
			return true;
		}
		catch (TargetInvocationException exception)
		{
			Console.WriteLine($"FAIL  {test.Name}: {exception.InnerException?.Message}");
			return false;
		}
	}
}
