using System;
using System.Numerics;
using Voxear;

namespace Hello
{
	/// <summary>Breathes another Obj's scale in and out, such as a lamp's bulb.</summary>
	public sealed class Pulse : Behaviour
	{
		[SerializeField] private Obj target;
		[SerializeField] private float amount = 0.15f;
		[SerializeField] private float speed = 2f;

		private Vector3 baseScale;
		private float phase;

		protected override void Start()
		{
			baseScale = target.Transform.Scale;
			phase = Random.Range(0f, Mathf.Tau);
		}

		protected override void Update()
		{
			if (!target.IsValid)
			{
				return;
			}

			var scale = 1f + MathF.Sin((float)Time.Elapsed * speed + phase) * amount;
			target.Transform = target.Transform with { Scale = baseScale * scale };
		}
	}
}
