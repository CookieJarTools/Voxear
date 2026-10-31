using System.Reflection;

namespace Voxear.Scripting
{
	/// <summary>Finds and runs the [Command] methods the editor's palette lists.</summary>
	internal static class Commands
	{
		private const BindingFlags Static = BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic;

		/// <summary>Runs the named method, and says whether it was there to run. What it throws reaches the caller.</summary>
		public static bool Run(string typeName, string methodName)
		{
			foreach (var assembly in GameScripts.Assemblies)
			{
				var method = assembly.GetType(typeName)?.GetMethod(methodName, Static, System.Type.EmptyTypes);
				if (method == null || method.GetCustomAttribute<CommandAttribute>() == null)
				{
					continue;
				}

				Invoke(method);
				return true;
			}
			return false;
		}

		/// <summary>Calls the method, handing on what it throws rather than the reflection wrapper around it.</summary>
		private static void Invoke(MethodInfo method)
		{
			try
			{
				method.Invoke(null, null);
			}
			catch (TargetInvocationException exception) when (exception.InnerException != null)
			{
				System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(exception.InnerException).Throw();
			}
		}
	}
}
