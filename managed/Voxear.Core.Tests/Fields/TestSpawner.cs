using System.Collections.Generic;

namespace Voxear.Tests;

internal sealed class TestSpawner : Behaviour
{
	public SpawnSettings Settings = new SpawnSettings();
	public List<Wave> Extra = new List<Wave>();
	public Obj Target;
	public int Lives = 3;
}
