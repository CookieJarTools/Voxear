using System.Numerics;

namespace Voxear.Tests;

/// <summary>Commands and variables, as a game's console finds them.</summary>
internal static class TestCheats
{
	[ConsoleVariable("Takes no damage")]
	public static bool God;

	[ConsoleVariable]
	public static float Gravity { get; set; } = -9.81f;

	public static string LastSpawn = string.Empty;

	[Command("Spawn Wave")]
	public static string SpawnWave(int count, MoveMode mode = MoveMode.Walk, string name = "Wolves")
	{
		LastSpawn = $"{count} {mode} {name}";
		return $"spawned {count}";
	}

	[Command("Teleport")]
	public static void Teleport(Vector3 to)
	{
		LastSpawn = $"at {to.X} {to.Y} {to.Z}";
	}
}
