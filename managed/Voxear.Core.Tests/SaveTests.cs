using System;
using System.Collections.Generic;
using System.Linq;
using System.Numerics;
using System.Text;
using Voxear.Saving;

namespace Voxear.Tests
{
	/// <summary>Saves away from the engine, in memory: what values keep, both formats, versions, converters and slots.</summary>
	internal static class SaveTests
	{
		[Test]
		private static void EveryKindOfValueComesBackInBothFormats()
		{
			var progress = FullProgress();
			foreach (var format in new[] { SaveFormat.Binary, SaveFormat.Json })
			{
				var bytes = format.Write(SaveNode.From(progress));
				CheckProgress(format.Read(bytes).As<TestProgress>(), format.Name);
			}
		}

		[Test]
		private static void WhatASaveLeavesOutStartsAsTheClassSetsIt()
		{
			var progress = FullProgress();
			progress.OnChanged = () => { };
			var node = SaveNode.From(progress);
			Check.True(!node.Has("Cached") && !node.Has("Scratch") && !node.Has("hidden"), "[DontSave], [NonSerialized] and plain private fields stay out");
			Check.True(!node.Has("OnChanged"), "a delegate is code, and stays out");
			Check.True(node.Has("secret") && node.Has("Coins") && node.Has("ShieldCount"), "[SerializeField], [field: SerializeField] and settable properties go in");

			var back = node.As<TestProgress>();
			Check.True(back.Cached == 0 && back.Hidden == 0 && back.OnChanged == null, "left out, so as a new one starts");
		}

		[Test]
		private static void JsonReadsAsAPersonWouldWriteIt()
		{
			var json = SaveFormat.Json.Write(SaveNode.From(new TestSettings { Volume = 0.5f, Subtitles = true }));
			var text = Encoding.UTF8.GetString(json);
			Check.True(text.Contains("\"Volume\": 0.5") && text.Contains("\"Language\": \"en\""), text);

			const string edited = """
				{
					// turned up by hand
					"Volume": 1,
					"Subtitles": false,
				}
				""";
			var settings = SaveFormat.Json.Read(Encoding.UTF8.GetBytes(edited)).As<TestSettings>();
			Check.Near(1f, settings.Volume);
			Check.True(settings.Language == "en", "a value the file leaves out keeps the class's own");
		}

		[Test]
		private static void BinaryIsSmallAndRefusesDamagedBytes()
		{
			var many = Enumerable.Range(0, 200).Select(i => new TestSettings { Volume = i / 200f }).ToList();
			var binary = SaveFormat.Binary.Write(SaveNode.From(many));
			var json = SaveFormat.Json.Write(SaveNode.From(many));
			Check.True(binary.Length * 3 < json.Length, $"binary {binary.Length} bytes against JSON's {json.Length}: each name is written once");

			foreach (var cut in new[] { 2, binary.Length / 2, binary.Length - 1 })
			{
				Throws<FormatException>(() => SaveFormat.Binary.Read(binary.AsSpan(0, cut)), $"cut at {cut}");
			}
			Throws<FormatException>(() => SaveFormat.Json.Read("{ not json"u8), "broken JSON");
		}

		[Test]
		private static void SharedAndCircularReferencesComeBackShared()
		{
			var anna = new TestChain { Name = "Anna" };
			var ben = new TestChain { Name = "Ben", Next = anna };
			anna.Next = ben;
			anna.Friends.Add(ben);
			ben.Friends.Add(ben);
			var group = new List<TestChain> { anna, ben, anna };

			foreach (var format in new[] { SaveFormat.Binary, SaveFormat.Json })
			{
				var back = format.Read(format.Write(SaveNode.From(group))).As<List<TestChain>>();
				Check.True(back.Count == 3 && ReferenceEquals(back[0], back[2]), $"{format}: the same object twice is one object");
				Check.True(ReferenceEquals(back[0].Next!.Next, back[0]), $"{format}: a loop is a loop");
				Check.True(ReferenceEquals(back[1].Friends[0], back[1]), $"{format}: an object can list itself");
			}

			var looped = new List<object>();
			looped.Add(looped);
			var thrown = Throws<SaveException>(() => SaveNode.From(looped), "a list inside itself");
			Check.True(thrown.Error == SaveError.Unsupported && thrown.Message.Contains("holds itself"), thrown.Message);
		}

		[Test]
		private static void MigrationsBringOldSavesUpToDateOneVersionAtATime()
		{
			var fromOne = Table(("hp", 40), ("Items", "rope,lamp"), ("Bag", Table(("size", 6))));
			var hero = fromOne.As<TestHero>();
			Check.True(hero.Health == 40, "version 1's hp became Health");
			Check.True(hero.Items.SequenceEqual(new[] { "rope", "lamp" }), "version 2's string became a list, after version 1's rename");
			Check.True(hero.Bag.Slots == 6, "a nested type migrates by its own version");
			Check.True(fromOne["$version"].AsInt() == 3, "marked up to date, so reading again changes nothing");
			Check.True(fromOne.As<TestHero>().Items.Count == 2, "read again, the same");

			var written = SaveNode.From(new TestHero { Health = 7 });
			Check.True(written["$version"].AsInt() == 3 && written["Bag"]["$version"].AsInt() == 2, "each marked type records its version");

			var fromFuture = Table(("$version", 4), ("Health", 1));
			var thrown = Throws<SaveException>(() => fromFuture.As<TestHero>(), "a newer save");
			Check.True(thrown.Error == SaveError.NewerVersion && thrown.Message.Contains("version 4") && thrown.Message.Contains("up to version 3"), thrown.Message);

			Save.AddMigration<TestSettings>(1, settings => settings.Rename("lang", "Language"));
			Check.True(Table(("lang", "fr")).As<TestSettings>().Language == "en", "an added migration waits for a version to migrate from");
		}

		[Test]
		private static void ConvertersAreFoundByThemselvesAndCanBeReplaced()
		{
			SaveTypes.Search(typeof(SaveTests).Assembly);
			var wallet = new TestWallet { Cash = new TestMoney(1234), History = { new TestMoney(5), new TestMoney(250) } };
			var node = SaveNode.From(wallet);
			Check.True(node["Cash"].AsString() == "12.34", $"the converter's text: {node}");
			var back = node.As<TestWallet>();
			Check.True(back.Cash.Cents == 1234 && back.History[1].Cents == 250, "read back through it");

			Save.AddConverter(new DelegateConverter<TestMoney>(money => money.Cents, read => new TestMoney(read.AsLong())));
			try
			{
				Check.True(SaveNode.From(wallet)["Cash"].AsLong() == 1234, "one added in code wins");
			}
			finally
			{
				Save.AddConverter(new TestMoneyConverter());
			}
		}

		[Test]
		private static void SlotsAreListedCopiedRenamedAndDeleted()
		{
			Save.Write("Slots A", new TestSettings { Volume = 0.1f });
			Save.Write("Slots B", new TestHero { Health = 9 }, "Chapter 2");
			var slots = Save.Slots.Where(slot => slot.Name.StartsWith("Slots ")).ToList();
			Check.True(slots.Select(slot => slot.Name).SequenceEqual(new[] { "Slots B", "Slots A" }), "newest first");
			Check.True(slots[0].Summary == "Chapter 2" && slots[0].Version == 3 && slots[0].Format == "json", slots[0].ToString());
			Check.True(slots[1].Summary == null && slots[1].Version == 1, "unmarked types are version 1");

			Check.True(Save.Read<TestHero>("slots b").Health == 9, "names match ignoring case");
			Save.Copy("Slots B", "Slots C");
			Save.Rename("Slots C", "Slots D");
			Check.True(!Save.Exists("Slots C") && Save.Read<TestHero>("Slots D").Health == 9, "copied, then renamed");
			Check.True(Save.Delete("Slots D") && !Save.Delete("Slots D"), "deleted once");

			var thrown = Throws<SaveException>(() => Save.Read<TestHero>("Slots D"), "a deleted slot");
			Check.True(thrown.Error == SaveError.Missing, thrown.Message);
			Check.True(Save.Read("Slots D", new TestSettings { Language = "de" }).Language == "de", "the fallback for a missing slot");
			Check.True(!Save.TryRead<TestHero>("Slots D", out _), "TryRead says no");
		}

		[Test]
		private static void AFileHoldsPartsAndFillsWhatAlreadyExists()
		{
			var spawner = new TestSpawner { Lives = 7, Settings = { Offset = new Vector3(1, 2, 3) } };
			var file = new SaveFile { Summary = "Wolves", Format = SaveFormat.Binary, Version = 5 };
			file.Set("spawner", spawner);
			file.Set("settings", new TestSettings { Subtitles = true });
			file.Write("Parts");
			spawner.Lives = 0;

			var read = SaveFile.Read("Parts");
			Check.True(read.Format == SaveFormat.Binary && read.Summary == "Wolves" && read.Version == 5, "what it says about itself");
			Check.True(read.Get<TestSettings>("settings").Subtitles, "a part by name");
			var fresh = new TestSpawner();
			read.ReadInto("spawner", fresh);
			Check.True(fresh.Lives == 7 && fresh.Settings.Offset == new Vector3(1, 2, 3), "a behaviour's fields, put back into one");
			Check.True(read.Get("missing", 42) == 42 && !read.Has("missing"), "a fallback for a missing part");
			Throws<SaveException>(() => read.Get<int>("missing"), "a missing part");

			var settings = new TestSettings { Language = "nl" };
			Save.Write("Partial", Table(("Volume", 0.25f)));
			Save.ReadInto("Partial", settings);
			Check.True(settings.Volume == 0.25f && settings.Language == "nl", "filling keeps what the save leaves out");
			Throws<SaveException>(() => Save.Read<TestSpawner>("Partial"), "a behaviour cannot be made by a save");
		}

		[Test]
		private static void ValuesThatDoNotFitKeepTheirDefaultsAndTheRestReads()
		{
			var hero = Table(("$version", 3), ("Health", "lots"), ("Items", Table(("oops", 1))), ("Bag", Table(("$version", 2), ("Slots", 4)))).As<TestHero>();
			Check.True(hero.Health == 100 && hero.Items.Count == 0, "the values that do not fit start as the class sets them");
			Check.True(hero.Bag.Slots == 4, "and the rest is read");

			var unknown = SaveNode.NewList(Table(("$type", "Voxear.Tests.Gone"), ("Name", "x")), Table(("Name", "kept")));
			var items = unknown.As<List<TestItem>>();
			Check.True(items.Count == 2 && items[0] == null && items[1].Name == "kept", "an item of a type the game no longer has is skipped");

			var thrown = Throws<SaveException>(() => SaveNode.From(new TestEntityHolder()), "an Entity");
			Check.True(thrown.Message.Contains("at Thing") && thrown.Message.Contains("Entity"), thrown.Message);
		}

		[Test]
		private static void AFormatOfTheGamesOwnIsFoundToReadWithIt()
		{
			SaveTypes.Search(typeof(SaveTests).Assembly);
			var file = new SaveFile { Format = new TestReversedFormat() };
			file.Set("settings", new TestSettings { Language = "ja" });
			file.Write("Reversed");
			var slot = Save.Slots.First(slot => slot.Name == "Reversed");
			Check.True(slot.Format == "reversed", slot.Format);
			Check.True(SaveFile.Read("Reversed").Get<TestSettings>("settings").Language == "ja", "read with the format it names");
		}

		[Test]
		private static void WritingWhileTheGameCarriesOnFinishes()
		{
			var write = Save.WriteAsync("Async", new TestSettings { Volume = 0.3f });
			var awaiter = write.GetAwaiter();
			Check.True(write.IsDone && awaiter.IsCompleted && write.Error == null, "memory writes at once");
			awaiter.GetResult();
			Check.Near(0.3f, Save.Read<TestSettings>("Async").Volume);

			var failed = Save.WriteAsync("bad/name", 1);
			Check.True(failed.IsDone && failed.Error != null, "a name that cannot be a file fails");
			Throws<SaveException>(() => failed.GetAwaiter().GetResult(), "awaiting it throws");
		}

		private static TestProgress FullProgress()
		{
			var progress = new TestProgress
			{
				Level = 12,
				Score = 9_000_000_000,
				Huge = ulong.MaxValue,
				Health = 0.1f,
				Distance = 1.0 / 3.0,
				Gold = 12.345m,
				Hardcore = true,
				Name = "Ünïcödé \"quoted\"",
				Grade = 'B',
				Difficulty = TestDifficulty.Hard,
				Perks = TestPerks.Swift | TestPerks.Lucky,
				Lives = 3,
				Look = new Vector2(0.5f, -1f),
				Position = new Vector3(1.5f, 2f, -3.25f),
				Tint4 = new Vector4(1, 2, 3, 4),
				Facing = Quaternion.CreateFromYawPitchRoll(0.3f, 0.2f, 0.1f),
				Tint = new Color(0.2f, 0.4f, 0.6f, 0.8f),
				Started = new DateTime(2026, 11, 1, 20, 30, 15, DateTimeKind.Utc),
				Played = TimeSpan.FromMinutes(83.5),
				Id = Guid.NewGuid(),
				Thumbnail = new byte[] { 0, 1, 254, 255 },
				Unlocked = new[] { 1, 4, 9 },
				Journal = { "woke", "walked" },
				Counts = { ["$type"] = 1, ["wolves"] = 3 },
				Best = { [TestDifficulty.Easy] = 12.5f },
				Notes = { [new Vector2(1, 2)] = "cave" },
				Seen = { 3, 5 },
				Next = new Queue<string>(new[] { "first", "second" }),
				Milestones = new List<int> { 10, 20 },
				Stats = new TestStats { Strength = 8, Speed = 1.25f },
				Items = { new TestItem { Name = "rope" }, new TestSword { Name = "blade", Damage = 12 } },
				Anything = 7,
				Review = new TestRecord("Lovely", 5),
				Pet = new Ref<Prefab>("713a4d989c654c6ca23b1d63201e0090"),
				Theme = new Ref<Sound>("0f2b6c1e9a8d4e7f8b1c2d3e4f5a6b7c"),
				ShieldCount = 2,
				Cached = 99,
				Scratch = 98,
			};
			progress.Undo.Push(1);
			progress.Undo.Push(2);
			progress.SetPrivate(41, 97, 250);
			return progress;
		}

		private static void CheckProgress(TestProgress back, string format)
		{
			string Say(string what) => $"{format}: {what}";
			Check.True(back.Level == 12 && back.Score == 9_000_000_000 && back.Huge == ulong.MaxValue, Say("whole numbers"));
			Check.True(back.Health == 0.1f && back.Distance == 1.0 / 3.0 && back.Gold == 12.345m, Say("numbers with fractions, exactly"));
			Check.True(back.Hardcore && back.Name == "Ünïcödé \"quoted\"" && back.Nickname == null && back.Grade == 'B', Say("text, letters and null"));
			Check.True(back.Difficulty == TestDifficulty.Hard && back.Perks == (TestPerks.Swift | TestPerks.Lucky), Say("enums and flags"));
			Check.True(back.Lives == 3, Say("a nullable"));
			Check.True(back.Look == new Vector2(0.5f, -1f) && back.Position == new Vector3(1.5f, 2f, -3.25f) && back.Tint4 == new Vector4(1, 2, 3, 4), Say("vectors"));
			Check.True(back.Facing == Quaternion.CreateFromYawPitchRoll(0.3f, 0.2f, 0.1f) && back.Tint == new Color(0.2f, 0.4f, 0.6f, 0.8f), Say("a rotation and a colour"));
			Check.True(back.Started == new DateTime(2026, 11, 1, 20, 30, 15, DateTimeKind.Utc) && back.Started.Kind == DateTimeKind.Utc, Say("a date"));
			Check.True(back.Played == TimeSpan.FromMinutes(83.5) && back.Id != Guid.Empty, Say("a time and a GUID"));
			Check.True(back.Thumbnail.SequenceEqual(new byte[] { 0, 1, 254, 255 }) && back.Unlocked.SequenceEqual(new[] { 1, 4, 9 }), Say("bytes and an array"));
			Check.True(back.Journal.SequenceEqual(new[] { "woke", "walked" }), Say("a list"));
			Check.True(back.Counts["$type"] == 1 && back.Counts["wolves"] == 3, Say("a dictionary, even with a key that looks like a mark"));
			Check.True(back.Best[TestDifficulty.Easy] == 12.5f && back.Notes[new Vector2(1, 2)] == "cave", Say("dictionaries keyed by enum and by vector"));
			Check.True(back.Seen.SetEquals(new[] { 3, 5 }) && back.Undo.Peek() == 2 && back.Undo.Count == 2, Say("a set, and a stack with the same top"));
			Check.True(back.Next.Peek() == "first" && back.Milestones.SequenceEqual(new[] { 10, 20 }), Say("a queue, and a list behind an interface"));
			Check.True(back.Stats.Strength == 8 && back.Stats.Speed == 1.25f, Say("a struct"));
			Check.True(back.Items[0].GetType() == typeof(TestItem) && back.Items[1] is TestSword { Name: "blade", Damage: 12 }, Say("a derived class in a list of its base"));
			Check.True(back.Anything is int and 7, Say("a boxed int stays an int"));
			Check.True(back.Review == new TestRecord("Lovely", 5), Say("a record without a constructor that takes nothing"));
			Check.True(!back.Home.IsValid && back.Pet.Path == "713a4d989c654c6ca23b1d63201e0090", Say("references"));
			Check.True(!back.Music.IsValid && back.Theme.Path == "0f2b6c1e9a8d4e7f8b1c2d3e4f5a6b7c", Say("a sound, and a reference to one"));
			Check.True(back.ShieldCount == 2 && back.Coins == 250 && back.Secret == 41, Say("properties, and private fields marked to save"));
		}

		private static SaveNode Table(params (string Name, SaveNode Value)[] fields)
		{
			var table = SaveNode.NewTable();
			foreach (var (name, value) in fields)
			{
				table[name] = value;
			}
			return table;
		}

		private static T Throws<T>(Action action, string what) where T : Exception
		{
			try
			{
				action();
			}
			catch (T expected)
			{
				return expected;
			}
			throw new Exception($"{what}: expected a {typeof(T).Name}");
		}
	}
}
