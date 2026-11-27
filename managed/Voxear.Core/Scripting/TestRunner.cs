using System;
using System.Collections.Generic;
using System.Reflection;
using Voxear.Testing;

namespace Voxear.Scripting;

/// <summary>
/// Runs the game's [Test] methods for the editor's Tests window: a test passes when it returns, and fails with what
/// an Assert said, or with what it threw and where.
/// </summary>
internal static class TestRunner
{
	private const BindingFlags Static = BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic;

	/// <summary>
	/// Runs the named test, and says whether there was one to run. failure is empty when it passed, and otherwise
	/// why it failed.
	/// </summary>
	public static bool Run(string typeName, string methodName, IEnumerable<Assembly> assemblies, out bool passed, out string failure)
	{
		(passed, failure) = (false, string.Empty);
		var method = Find(typeName, methodName, assemblies);
		if (method == null)
		{
			return false;
		}

		try
		{
			method.Invoke(null, null);
			passed = true;
		}
		catch (TargetInvocationException exception) when (exception.InnerException != null)
		{
			failure = Why(exception.InnerException);
		}
		return true;
	}

	/// <summary>Whether a method is a test: static, marked [Test], taking nothing and returning nothing.</summary>
	public static bool IsTest(MethodInfo method)
	{
		return method.IsStatic
			&& method.GetCustomAttribute<TestAttribute>() != null
			&& method.GetParameters().Length == 0
			&& method.ReturnType == typeof(void);
	}

	private static MethodInfo? Find(string typeName, string methodName, IEnumerable<Assembly> assemblies)
	{
		foreach (var assembly in assemblies)
		{
			var method = assembly.GetType(typeName)?.GetMethod(methodName, Static, Type.EmptyTypes);
			if (method != null && IsTest(method))
			{
				return method;
			}
		}
		return null;
	}

	/// <summary>An Assert's own message; anything else thrown with its type and the line it came from.</summary>
	private static string Why(Exception thrown)
	{
		if (thrown is AssertionException failed)
		{
			return failed.Message;
		}
		var trace = thrown.StackTrace ?? string.Empty;
		var lines = trace.Split('\n', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
		var at = lines.Length > 0 ? $"\n{Short(lines[0])}" : string.Empty;
		return $"{thrown.GetType().Name}: {thrown.Message}{at}";
	}

	/// <summary>A stack trace's line with its file by name alone: "at Game.Tests.Run() in Tests.cs:line 24".</summary>
	private static string Short(string line)
	{
		var at = line.LastIndexOf(" in ", StringComparison.Ordinal);
		if (at < 0)
		{
			return line;
		}
		var place = line[(at + 4)..];
		var slash = place.LastIndexOfAny(['/', '\\']);
		return slash < 0 ? line : $"{line[..(at + 4)]}{place[(slash + 1)..]}";
	}
}
