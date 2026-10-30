using System;
using System.Linq;
using System.Reflection;

namespace Voxear.Tests
{
	/// <summary>Runs every [Test] method in this assembly and exits non-zero when any fails.</summary>
	internal static class Program
	{
		private static int Main()
		{
			var tests = typeof(Program).Assembly.GetTypes()
				.SelectMany(type => type.GetMethods(BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic))
				.Where(method => method.IsDefined(typeof(TestAttribute)))
				.OrderBy(method => method.DeclaringType!.Name).ThenBy(method => method.Name)
				.ToList();

			var failed = tests.Count(method => !Run(method));
			Console.WriteLine($"{tests.Count - failed} passed, {failed} failed");
			return failed == 0 ? 0 : 1;
		}

		private static bool Run(MethodInfo test)
		{
			var name = $"{test.DeclaringType!.Name}.{test.Name}";
			try
			{
				test.Invoke(null, null);
				Console.WriteLine($"ok    {name}");
				return true;
			}
			catch (TargetInvocationException exception)
			{
				Console.WriteLine($"FAIL  {name}: {exception.InnerException?.Message}");
				return false;
			}
		}
	}
}
