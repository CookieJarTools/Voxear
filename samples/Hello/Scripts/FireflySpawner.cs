using System;
using System.Numerics;
using Voxear;

namespace Hello
{
	/// <summary>
	/// Releases fireflies around its Obj from a prefab it loads once. They are created at runtime, so they belong
	/// to the Night scene and go when it unloads.
	/// </summary>
	public sealed class FireflySpawner : Behaviour
	{
		private const float Interval = 0.25f;
		private const float Spread = 3f;

		private Prefab firefly;
		private float timer;
		private int released;

		protected override void Awake()
		{
			firefly = Prefab.Load("Prefabs/Firefly.prefab");
		}

		protected override void Update()
		{
			timer += Time.Delta;
			if (timer < Interval)
			{
				return;
			}

			timer = 0f;
			var offset = new Vector3(Random.Shared.NextSingle() - 0.5f, 0.1f, Random.Shared.NextSingle() - 0.5f) * Spread;
			var copy = firefly.Instantiate(Obj.Position + offset, Quaternion.Identity);
			released++;
			if (released == 1)
			{
				Log.Info($"Instantiated {copy.Name}; its OnEnable had already run: {copy.GetBehaviour<Firefly>()?.Ready}");
			}
		}
	}
}
