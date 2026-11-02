using System;
using System.Collections.Generic;
using System.Reflection;
using System.Text;

namespace Voxear.Scripting;

/// <summary>
/// Runs the checks a VoxDocs guide's steps name: a static method of the game's scripts that takes nothing and
/// returns bool, true when it passes, or string, null or empty when it passes and otherwise what is still wrong.
/// The editor asks every second or so while a guide with one shows.
/// </summary>
internal static class GuideChecks
{
	private const BindingFlags Static = BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic;

	/// <summary>
	/// Runs the named check against the game's scripts, and says whether there was one to run. failure is what is
	/// still wrong, empty when the check passed or said nothing. What it throws reaches the caller.
	/// </summary>
	public static bool Run(string typeName, string methodName, out bool passed, out string failure)
	{
		return Run(typeName, methodName, GameScripts.Assemblies, out passed, out failure);
	}

	public static bool Run(string typeName, string methodName, IEnumerable<Assembly> assemblies, out bool passed, out string failure)
	{
		(passed, failure) = (false, string.Empty);
		var method = Find(typeName, methodName, assemblies);
		if (method == null)
		{
			return false;
		}

		var answer = Invoke(method);
		failure = answer as string ?? string.Empty;
		passed = answer is bool ok ? ok : failure.Length == 0;
		return true;
	}

	/// <summary>Writes text into buffer, cut short at a character's edge when it does not fit, and returns its length.</summary>
	public static int Write(string text, Span<byte> buffer)
	{
		var bytes = Encoding.UTF8.GetBytes(text);
		var length = Math.Min(bytes.Length, buffer.Length);
		// A byte of 10xxxxxx continues a character, so the cut moves back to where one starts.
		while (length < bytes.Length && length > 0 && (bytes[length] & 0xC0) == 0x80)
		{
			length--;
		}
		bytes.AsSpan(0, length).CopyTo(buffer);
		return length;
	}

	private static MethodInfo? Find(string typeName, string methodName, IEnumerable<Assembly> assemblies)
	{
		foreach (var assembly in assemblies)
		{
			var method = assembly.GetType(typeName)?.GetMethod(methodName, Static, Type.EmptyTypes);
			if (method != null && (method.ReturnType == typeof(bool) || method.ReturnType == typeof(string)))
			{
				return method;
			}
		}
		return null;
	}

	/// <summary>Calls the method, handing on what it throws rather than the reflection wrapper around it.</summary>
	private static object? Invoke(MethodInfo method)
	{
		try
		{
			return method.Invoke(null, null);
		}
		catch (TargetInvocationException exception) when (exception.InnerException != null)
		{
			System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(exception.InnerException).Throw();
			throw;
		}
	}
}
