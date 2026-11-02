using System.Collections.Generic;

namespace Voxear.Tests;

[Saved]
internal sealed class TestChest : Behaviour
{
	public bool Looted;
	public List<string> Items = new List<string> { "potion" };
	private int hidden;

	public int Hidden => hidden;
}
