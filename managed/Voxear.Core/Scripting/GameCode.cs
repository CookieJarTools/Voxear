using System;
using System.Runtime.Loader;

namespace Voxear.Scripting;

/// <summary>
/// Tells whether something comes from one load of the game scripts. A type made from a generic one counts when
/// any of its arguments does: a List of a script's class lives in the core library, but keeps that load of the
/// scripts in memory all the same, and so does the box an async method of theirs waits in.
/// </summary>
internal static class GameCode
{
	public static bool IsFrom(Type type, AssemblyLoadContext context)
	{
		if (type.HasElementType)
		{
			return IsFrom(type.GetElementType()!, context);
		}
		if (AssemblyLoadContext.GetLoadContext(type.Assembly) == context)
		{
			return true;
		}
		return type.IsConstructedGenericType && Array.Exists(type.GetGenericArguments(), argument => IsFrom(argument, context));
	}

	/// <summary>Whether keeping or running this would reach into that load: a delegate by each method it calls and what it calls them on, anything else by its type.</summary>
	public static bool Holds(object? value, AssemblyLoadContext context)
	{
		if (value is not Delegate handler)
		{
			return value != null && IsFrom(value.GetType(), context);
		}
		foreach (var single in handler.GetInvocationList())
		{
			if (Calls(single, context))
			{
				return true;
			}
		}
		return false;
	}

	/// <summary>A handler without the listeners that reach into that load, or null when none is left.</summary>
	public static T? Without<T>(T? handler, AssemblyLoadContext context) where T : Delegate
	{
		if (handler == null)
		{
			return null;
		}

		Delegate? kept = null;
		foreach (var listener in handler.GetInvocationList())
		{
			if (!Calls(listener, context))
			{
				kept = Delegate.Combine(kept, listener);
			}
		}
		return (T?)kept;
	}

	private static bool Calls(Delegate single, AssemblyLoadContext context)
	{
		var owner = single.Method.DeclaringType;
		return (owner != null && IsFrom(owner, context)) || Holds(single.Target, context);
	}
}
