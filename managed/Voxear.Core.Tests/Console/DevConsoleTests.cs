using System.Linq;
using Voxear.Scripting;

namespace Voxear.Tests
{
	internal static class DevConsoleTests
	{
		private static string Printed(string line)
		{
			var printed = DevConsole.Run(line, new[] { typeof(TestCheats).Assembly });
			return printed == null ? "<none>" : string.Join("\n", printed.Select(p => p.Text));
		}

		[Test]
		private static void CommandsRunWithTheirArguments()
		{
			Check.True(Printed("spawnwave 3 fly \"Grey Wolves\"") == "spawned 3", Printed("spawnwave 3 fly \"Grey Wolves\""));
			Check.True(TestCheats.LastSpawn == "3 Fly Grey Wolves", TestCheats.LastSpawn);
			Printed("SpawnWave 2");
			Check.True(TestCheats.LastSpawn == "2 Walk Wolves", "missing arguments take their defaults");
			Printed("TestCheats.Teleport 1,2.5,-3");
			Check.True(TestCheats.LastSpawn == "at 1 2.5 -3", TestCheats.LastSpawn);
			Check.True(Printed("SpawnWave lots").Contains("is not a whole number"), Printed("SpawnWave lots"));
			Check.True(Printed("SpawnWave").Contains("takes count"), "too few arguments say what it takes");
			Check.True(Printed("nothing here") == "<none>", "an unknown name is left to the caller");
		}

		[Test]
		private static void VariablesReadAndSet()
		{
			Check.True(Printed("god") == "God = False", Printed("god"));
			Check.True(Printed("god on") == "God = True" && TestCheats.God, "set with on");
			Check.True(Printed("Gravity -3.5") == "Gravity = -3.5", Printed("Gravity -3.5"));
			Check.True(Printed("god maybe").Contains("is not true or false"), Printed("god maybe"));
			var help = Printed("help");
			Check.True(help.Contains("SpawnWave count [mode] [name]  Spawn Wave") && help.Contains("God = True  Takes no damage"), help);
		}
	}
}
