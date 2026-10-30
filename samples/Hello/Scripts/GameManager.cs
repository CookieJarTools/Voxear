using Voxear;

namespace Hello
{
	/// <summary>
	/// Keeps score and alternates between the Day and Night levels every few seconds, loading each in the background
	/// with Level.LoadAsync and carrying on once it is in. It is an IInstance,
	/// so any script reaches it through Instance&lt;GameManager&gt;.Current, and its Obj is persistent, so it
	/// survives every level change while the scenes around it come and go.
	/// </summary>
	[ExecutionOrder(-100)]
	public sealed class GameManager : Behaviour, IInstance
	{
		private const float SecondsPerLevel = 5f;

		public int Score { get; private set; }

		private float timer;
		private bool night;
		private LevelLoad? loading;

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

		protected override void Update()
		{
			timer += Time.Delta;
			if (timer < SecondsPerLevel || loading != null)
			{
				return;
			}

			timer = 0f;
			night = !night;
			SwitchLevel(night ? "Levels/Night.level" : "Levels/Day.level");
		}

		private async void SwitchLevel(string level)
		{
			loading = Level.LoadAsync(level);
			await loading;
			Log.Info($"{level} is in after loading in the background{(loading.Error == null ? "" : $", but: {loading.Error}")}");
			Log.Info($"Loaded now: {Content.Usage}");
			loading = null;
		}

		protected override void OnDestroy()
		{
			Log.Info($"OnDestroy {Obj.Name} (GameManager) - should never happen, it is persistent");
		}
	}
}
