using System;
using System.Numerics;
using Voxear;

namespace Hello
{
	/// <summary>
	/// Releases fireflies around its Obj. The common one is a Prefab field, loaded with the scene before Awake. The
	/// rare one is a Ref&lt;Prefab&gt;, loaded the first time it is picked. Fireflies are created at runtime, so they
	/// belong to the Night scene and go when it unloads.
	/// </summary>
	public sealed class FireflySpawner : Behaviour
	{
		[SerializeField] private Prefab firefly;
		[SerializeField] private Ref<Prefab> rareFirefly;
		[SerializeField] private float interval = 0.25f;
		[SerializeField] private float spread = 3f;
		[SerializeField] private float rareChance = 0.1f;

		private Prefab rare;
		private float timer;
		private int released;

		protected override void Update()
		{
			timer += Time.Delta;
			if (timer < interval)
			{
				return;
			}

			timer = 0f;
			var offset = new Vector3(Random.Range(-0.5f, 0.5f), 0.03f, Random.Range(-0.5f, 0.5f)) * spread;
			var copy = Choose().Instantiate(Obj.WorldPosition + offset, Quaternion.Identity);
			released++;
			if (released == 1)
			{
				Log.Info($"Instantiated {copy.Name}; its OnEnable had already run: {copy.GetBehaviour<Firefly>()?.Ready}");
			}
		}

		private Prefab Choose()
		{
			if (!Random.Chance(rareChance))
			{
				return firefly;
			}

			if (!rare.IsValid)
			{
				rare = rareFirefly.Load();
				Log.Info($"Loaded {rareFirefly.Path} the first time it was needed");
			}
			return rare;
		}
	}
}
