using System.Numerics;
using Voxear;

namespace Hello
{
	/// <summary>
	/// Spins its Obj around the up axis and checks in with the GameManager when it starts. Given a leader, it spins
	/// the other way at the leader's speed.
	/// </summary>
	public sealed class Spinner : Behaviour
	{
		[SerializeField] private float degreesPerSecond = 90f;
		[SerializeField] private Spinner? leader;

		public float DegreesPerSecond => leader != null ? -leader.degreesPerSecond : degreesPerSecond;

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
			Obj.Rotation = Quaternion.Normalize(Obj.Rotation * Quaternion.AngleAxis(DegreesPerSecond * Time.Delta, Vector3.Up));
		}
	}
}
