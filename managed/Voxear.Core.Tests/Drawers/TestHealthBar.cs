using System.Collections.Generic;

namespace Voxear.Tests;

internal sealed class TestHealthBar : Behaviour
{
	public TestHealth Health = new TestHealth { Current = 3, Max = 5 };

	[TestTag]
	public TestHealth Shield = new TestHealth { Max = 5 };

	[TestStep(2)]
	public int Roll = 1;

	[TestStep(2)]
	public string Name = string.Empty;

	public List<TestHealth> Extras = new List<TestHealth>();
}
