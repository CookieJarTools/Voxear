using System;
using System.Numerics;
using Voxear;

namespace Hello
{
	/// <summary>Spins its Obj around the up axis and checks in with the GameManager when it starts.</summary>
	public sealed class Spinner : Behaviour
	{
		private const float DegreesPerSecond = 90f;

		protected override void Awake()
		{
			Log.Info($"Awake    {Obj.Name} (Spinner)");
		}

		protected override void OnEnable()
		{
			Log.Info($"OnEnable {Obj.Name} (Spinner)");
		}

		protected override void Start()
		{
			var manager = Instance<GameManager>.Current;
			manager?.AddScore(1);
			Log.Info($"Start    {Obj.Name} (Spinner), score now {manager?.Score}");
		}

		protected override void Update()
		{
			var step = Quaternion.CreateFromAxisAngle(Vector3.UnitY, DegreesPerSecond * MathF.PI / 180f * Time.Delta);
			Obj.Rotation = Quaternion.Normalize(Obj.Rotation * step);
		}
	}
}
