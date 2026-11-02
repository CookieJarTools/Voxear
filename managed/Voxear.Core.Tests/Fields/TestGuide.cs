namespace Voxear.Tests;

[Documentation("Combat/Damage")]
internal sealed class TestGuide : Behaviour
{
	[Documentation("Combat/Damage#hits")]
	[SerializeField] private int damage = 10;

	[Excerpt("Combat/Damage#falloff")]
	[SerializeField] private float falloff = 2;
}
