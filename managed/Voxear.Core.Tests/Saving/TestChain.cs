using System.Collections.Generic;

namespace Voxear.Tests
{
	internal sealed class TestChain
	{
		public string Name = string.Empty;
		public TestChain? Next;
		public List<TestChain> Friends = new List<TestChain>();
	}
}
