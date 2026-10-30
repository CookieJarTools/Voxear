using Voxear;

namespace Hello
{
	/// <summary>
	/// Keeps score for the demo. It is an IInstance, so any script reaches it through
	/// Instance&lt;GameManager&gt;.Current, and it runs before other behaviours in every phase.
	/// </summary>
	[ExecutionOrder(-100)]
	public sealed class GameManager : Behaviour, IInstance
	{
		public int Score { get; private set; }

		public void AddScore(int amount)
		{
			Score += amount;
		}

		protected override void Awake()
		{
			Log.Info($"Awake    {Obj.Name} (GameManager)");
		}

		protected override void OnEnable()
		{
			Log.Info($"OnEnable {Obj.Name} (GameManager)");
		}

		protected override void Start()
		{
			Log.Info($"Start    {Obj.Name} (GameManager), score {Score}");
		}
	}
}
