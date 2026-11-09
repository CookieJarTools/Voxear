namespace Voxear.Tests;

internal static class TestRifleObjects
{
	[CreateObject("Guns/Rifle")]
	private static readonly Ref<Prefab> Rifle = new("Prefabs/Rifle.prefab");

	[CreateObject("Guns/Unnamed")]
	private static readonly Ref<Prefab> Unnamed = default;

	[CreateObject("Guns/Pistol")]
	public static Ref<Prefab> Pistol => new("Prefabs/Pistol.prefab");
}
