using System;
using System.Numerics;
using Voxear;

namespace Hello
{
	/// <summary>Drifts upward with a little wander, then destroys its own Obj.</summary>
	public sealed class Firefly : Behaviour
	{
		private const float Lifetime = 3f;
		private const float RiseSpeed = 0.35f;
		private const float WanderSpeed = 0.25f;

		/// <summary>Set in OnEnable, which runs before Instantiate returns.</summary>
		public bool Ready { get; private set; }

		private float age;
		private Vector3 wander;
		private bool leaving;

		protected override void Awake()
		{
			var angle = Random.Shared.NextSingle() * MathF.Tau;
			wander = new Vector3(MathF.Cos(angle), 0f, MathF.Sin(angle)) * WanderSpeed;
		}

		protected override void OnEnable()
		{
			Ready = true;
		}

		protected override void Update()
		{
			age += Time.Delta;
			Obj.Position += (Vector3.UnitY * RiseSpeed + wander) * Time.Delta;
			if (age < Lifetime || leaving)
			{
				return;
			}

			leaving = true;
			Obj.Destroy();
		}
	}
}
