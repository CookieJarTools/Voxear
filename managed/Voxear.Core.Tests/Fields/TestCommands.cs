namespace Voxear.Tests
{
	/// <summary>Commands for the catalog to find: one it lists, and two it leaves out as the palette cannot run them.</summary>
	internal static class TestCommands
	{
		[Command("Tidy Up", Keys = "Ctrl+Shift+T")]
		private static void TidyUp()
		{
		}

		[Command("Takes Something")]
		private static void TakesSomething(int amount)
		{
		}

		[Command("Gives Something")]
		private static int GivesSomething()
		{
			return 0;
		}
	}
}
