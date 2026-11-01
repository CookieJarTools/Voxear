using System;
using Voxear;

namespace Hello
{
	/// <summary>
	/// Keeps score and moves through the levels its scene lists in cycle, loading each in the background with
	/// Level.LoadAsync and carrying on once it is in. Day and Night both stand on the World level, so the Environment
	/// scene stays loaded between them. It is an IInstance,
	/// so any script reaches it through Instance&lt;GameManager&gt;.Current, and its Obj is persistent, so it
	/// survives every level change while the scenes around it come and go.
	/// </summary>
	[ExecutionOrder(-100)]
	[Documentation("How Hello Works/The Day and Night Cycle")]
	public sealed class GameManager : Behaviour, IInstance
	{
		[Excerpt("How Hello Works/The Day and Night Cycle#who-drives-it")]
		[SerializeField] private LevelStop[] cycle = Array.Empty<LevelStop>();

		public int Score { get; private set; }

		private float timer;
		private int next;
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
			if (cycle.Length == 0 || timer < cycle[next].Seconds || loading != null)
			{
				return;
			}

			timer = 0f;
			SwitchLevel(cycle[next].Level);
			next = (next + 1) % cycle.Length;
		}

		private async void SwitchLevel(string level)
		{
			loading = Level.LoadAsync(level);
			try
			{
				await loading;
				Log.Info($"{level} is in after loading in the background, bringing in {string.Join(", ", loading.Scenes)}");
			}
			catch (LevelLoadException exception)
			{
				Log.Error(exception.Message);
			}

			Log.Info($"Loaded now: {Content.Usage}");
			loading = null;
		}

		protected override void OnDestroy()
		{
			Log.Info($"OnDestroy {Obj.Name} (GameManager) - should never happen, it is persistent");
		}
	}
}
