namespace Voxear.Tests
{
	/// <summary>Code that loads assets by path, and says so.</summary>
	[ContentRoot("Prefabs/Bosses/**", "Levels/Arena.level")]
	internal sealed class BossSpawner
	{
		[ContentRoot("Prefabs/Minion.prefab")]
		private readonly int minions = 0;

		[ContentRoot("Materials/Rage.material")]
		private void Enrage()
		{
		}

		public int Minions => minions;
	}
}
