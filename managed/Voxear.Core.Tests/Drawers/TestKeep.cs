namespace Voxear.Tests;

internal sealed class TestKeep : Behaviour
{
	public int Floors = 2;

	public TestMeter Stores = new TestMeter { Current = 1, Max = 4 };
}
