using System;
using Voxear.Interop;

namespace Voxear
{
	/// <summary>Writes to the engine log, which the editor console and the CLI both show.</summary>
	public static class Log
	{
		public static void Debug(string message)
		{
			Native.Log(LogLevel.Debug, message);
		}

		public static void Info(string message)
		{
			Native.Log(LogLevel.Info, message);
		}

		public static void Warning(string message)
		{
			Native.Log(LogLevel.Warning, message);
		}

		public static void Error(string message)
		{
			Native.Log(LogLevel.Error, message);
		}

		public static void Exception(Exception exception, Behaviour? context = null)
		{
			var source = context == null ? string.Empty : $" in {context.GetType().Name} on {context.Obj.Name}";
			Native.Log(LogLevel.Error, $"{exception.GetType().Name}{source}: {exception.Message}\n{exception.StackTrace}");
		}
	}
}
