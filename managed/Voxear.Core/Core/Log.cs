using System;
using System.Diagnostics;
using Voxear.Interop;

namespace Voxear
{
	/// <summary>
	/// Writes to the engine log, which the editor Console and the CLI both show. Each line carries the stack that wrote
	/// it, as Unity's logs do, so the Console can say where it came from and open any frame of it; an exception carries
	/// its own trace, and the Obj it happened on.
	/// </summary>
	public static class Log
	{
		public static void Debug(string message)
		{
			Write(LogLevel.Debug, message);
		}

		public static void Info(string message)
		{
			Write(LogLevel.Info, message);
		}

		public static void Warning(string message)
		{
			Write(LogLevel.Warning, message);
		}

		public static void Error(string message)
		{
			Write(LogLevel.Error, message);
		}

		public static void Exception(Exception exception, Behaviour? context = null)
		{
			var source = context == null ? string.Empty : $" in {context.GetType().Name} on {context.Obj.Name}";
			var message = $"{exception.GetType().Name}{source}: {exception.Message}";
			Native.Log(LogLevel.Error, message, exception.StackTrace ?? string.Empty, context?.Obj.Id ?? 0);
		}

		/// <summary>Writes a line with its caller's stack: the two frames skipped are this method and the one above it.</summary>
		private static void Write(LogLevel level, string message)
		{
			Native.Log(level, message, new StackTrace(2, true).ToString(), 0);
		}
	}
}
