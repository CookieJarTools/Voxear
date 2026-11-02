using System;
using System.Diagnostics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// Writes to the engine log, which the editor Console and the CLI both show. Each line carries the stack that wrote
/// it, as Unity's logs do, so the Console can say where it came from and open any frame of it; an exception carries
/// its own trace, and the Obj it happened on. A line can say what it is about, as an Obj, a behaviour or an asset,
/// and clicking it in the Console shows that thing.
/// </summary>
public static class Log
{
	public static void Debug(string message)
	{
		Write(LogLevel.Debug, message, default);
	}

	/// <summary>A debug line about an Obj, a behaviour or an asset, which clicking the line shows.</summary>
	public static void Debug(string message, LogContext context)
	{
		Write(LogLevel.Debug, message, context);
	}

	public static void Info(string message)
	{
		Write(LogLevel.Info, message, default);
	}

	/// <summary>A line about an Obj, a behaviour or an asset, which clicking the line shows.</summary>
	public static void Info(string message, LogContext context)
	{
		Write(LogLevel.Info, message, context);
	}

	public static void Warning(string message)
	{
		Write(LogLevel.Warning, message, default);
	}

	/// <summary>A warning about an Obj, a behaviour or an asset, which clicking the line shows.</summary>
	public static void Warning(string message, LogContext context)
	{
		Write(LogLevel.Warning, message, context);
	}

	public static void Error(string message)
	{
		Write(LogLevel.Error, message, default);
	}

	/// <summary>An error about an Obj, a behaviour or an asset, which clicking the line shows.</summary>
	public static void Error(string message, LogContext context)
	{
		Write(LogLevel.Error, message, context);
	}

	public static void Exception(Exception exception, Behaviour? context = null)
	{
		var source = context == null ? string.Empty : $" in {context.GetType().Name} on {context.Obj.Name}";
		var message = $"{exception.GetType().Name}{source}: {exception.Message}";
		Native.Log(LogLevel.Error, message, exception.StackTrace ?? string.Empty, context);
	}

	/// <summary>Writes a line with its caller's stack: the two frames skipped are this method and the one above it.</summary>
	private static void Write(LogLevel level, string message, LogContext context)
	{
		Native.Log(level, message, new StackTrace(2, true).ToString(), context);
	}
}
