using System.Numerics;
using Voxear;

namespace Hello
{
	/// <summary>Spins its Obj around the up axis and checks in with the GameManager when it starts.</summary>
	public sealed class Spinner : Behaviour
	{
		[SerializeField] private float degreesPerSecond = 90f;

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

		protected override void OnDestroy()
		{
			Log.Info($"OnDestroy {Obj.Name} (Spinner), score was {Instance<GameManager>.Current?.Score}");
		}

		protected override void Update()
		{
			Obj.Rotation = Quaternion.Normalize(Obj.Rotation * Quaternion.AngleAxis(degreesPerSecond * Time.Delta, Vector3.Up));
		}
	}
}
