using System;
using System.Collections.Generic;
using System.Numerics;

namespace Voxear.Tests
{
	/// <summary>One of everything a save keeps.</summary>
	internal sealed class TestProgress
	{
		public int Level;
		public long Score;
		public ulong Huge;
		public float Health;
		public double Distance;
		public decimal Gold;
		public bool Hardcore;
		public string Name = string.Empty;
		public string? Nickname;
		public char Grade;
		public TestDifficulty Difficulty;
		public TestPerks Perks;
		public int? Lives;
		public Vector2 Look;
		public Vector3 Position;
		public Vector4 Tint4;
		public Quaternion Facing;
		public Color Tint;
		public DateTime Started;
		public TimeSpan Played;
		public Guid Id;
		public byte[] Thumbnail = Array.Empty<byte>();
		public int[] Unlocked = Array.Empty<int>();
		public List<string> Journal = new List<string>();
		public Dictionary<string, int> Counts = new Dictionary<string, int>();
		public Dictionary<TestDifficulty, float> Best = new Dictionary<TestDifficulty, float>();
		public Dictionary<Vector2, string> Notes = new Dictionary<Vector2, string>();
		public HashSet<int> Seen = new HashSet<int>();
		public Stack<int> Undo = new Stack<int>();
		public Queue<string> Next = new Queue<string>();
		public IReadOnlyList<int> Milestones = new List<int>();
		public TestStats Stats;
		public List<TestItem> Items = new List<TestItem>();
		public object? Anything;
		public TestRecord? Review;
		public Obj Home;
		public Ref<Prefab> Pet;
		public Sound Music;
		public Ref<Sound> Theme;
		public Action? OnChanged;
		public int ShieldCount { get; set; }

		[field: SerializeField]
		public int Coins { get; private set; }

		[DontSave]
		public int Cached;

		[NonSerialized]
		public int Scratch;

		[SerializeField]
		private int secret;

		private int hidden;

		public int Secret => secret;

		public int Hidden => hidden;

		public void SetPrivate(int secretValue, int hiddenValue, int coins)
		{
			secret = secretValue;
			hidden = hiddenValue;
			Coins = coins;
		}
	}
}
