using System.Collections.Generic;

namespace Voxear.Tests;

internal sealed class TestWallet
{
	public TestMoney Cash;
	public List<TestMoney> History = new List<TestMoney>();
}
