using System;
using System.Numerics;
using Voxear;

namespace Hello
{
	/// <summary>
	/// Releases fireflies around the lamps it is given, or around itself without any. The common one is a Prefab field,
	/// loaded with the scene before Awake. The rare one is a Ref&lt;Prefab&gt; loaded in the background from Awake, and
	/// common ones stand in until it arrives. Fireflies are created at runtime, so they belong to the Night scene and
	/// go when it unloads.
	/// </summary>
	public sealed class FireflySpawner : Behaviour
	{
		[SerializeField] private Prefab firefly;
		[SerializeField] private Ref<Prefab> rareFirefly;
		[SerializeField] private Obj[] lamps = Array.Empty<Obj>();
		[SerializeField] private float interval = 0.25f;
		[SerializeField] private float spread = 3f;
		[SerializeField] private float rareChance = 0.1f;

		private PrefabLoad? rare;
		private float timer;
		private int released;

		protected override void Awake()
		{
			rare = Prefab.LoadAsync(rareFirefly.Path);
		}

		protected override void Update()
		{
			timer += Time.Delta;
			if (timer < interval)
			{
				return;
			}

			timer = 0f;
			var offset = new Vector3(Random.Range(-0.5f, 0.5f), 0.03f, Random.Range(-0.5f, 0.5f)) * spread;
			var copy = Choose().Instantiate(Origin() + offset, Quaternion.Identity);
			released++;
			if (released == 1)
			{
				Log.Info($"Instantiated {copy.Name}; its OnEnable had already run: {copy.GetBehaviour<Firefly>()?.Ready}");
			}
		}

		private Vector3 Origin()
		{
			return lamps.Length == 0 ? Obj.WorldPosition : lamps[Random.Range(0, lamps.Length)].WorldPosition.WithY(0f);
		}

		private Prefab Choose()
		{
			var ready = rare != null && rare.IsDone && rare.Prefab.IsValid;
			return ready && Random.Chance(rareChance) ? rare!.Prefab : firefly;
		}
	}
}
