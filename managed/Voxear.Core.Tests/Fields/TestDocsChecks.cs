namespace Voxear.Tests
{
	/// <summary>Guide checks for the catalog to find: two it lists, and one it leaves out as it takes something.</summary>
	internal static class TestDocsChecks
	{
		[DocsCheck("The door is shut")]
		private static bool DoorIsShut()
		{
			return true;
		}

		[DocsCheck("The lamps are lit")]
		private static string? LampsAreLit()
		{
			return null;
		}

		[DocsCheck("Takes Something")]
		private static bool TakesSomething(int amount)
		{
			return amount > 0;
		}
	}
}
