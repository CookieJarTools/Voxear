using System;
using System.Collections.Generic;
using System.Globalization;

namespace Voxear.Saving
{
	/// <summary>
	/// Keys for the Objs scripts made from prefabs. The engine names one by its copy's root entity, which holds only
	/// for this run, so a save of the scene numbers the copies it keeps instead, and reading it back turns each number
	/// into the copy made again. Outside a save of the scene such an Obj has no key a save can keep.
	/// </summary>
	internal sealed class SpawnedKeys
	{
		public const string Prefix = "spawned:";

		[ThreadStatic]
		private static SpawnedKeys? current;

		/// <summary>While a save of the scene is written: each copy's root entity, and its number in the save.</summary>
		private readonly Dictionary<ulong, int>? numbers;

		/// <summary>While one is read back: each numbered copy, made again.</summary>
		private readonly IReadOnlyList<Obj>? made;

		private SpawnedKeys(Dictionary<ulong, int>? numbers, IReadOnlyList<Obj>? made)
		{
			this.numbers = numbers;
			this.made = made;
		}

		public static IDisposable Writing(Dictionary<ulong, int> numbers)
		{
			var outer = current;
			current = new SpawnedKeys(numbers, null);
			return new Scope(() => current = outer);
		}

		public static IDisposable Reading(IReadOnlyList<Obj> made)
		{
			var outer = current;
			current = new SpawnedKeys(null, made);
			return new Scope(() => current = outer);
		}

		/// <summary>The key a save keeps for an Obj, from the engine's: as it is for an Obj in a scene, numbered for a copy, or null.</summary>
		public static string? ForSave(string? key)
		{
			if (key == null || !key.StartsWith(Prefix, StringComparison.Ordinal))
			{
				return key;
			}
			if (current?.numbers == null || !Split(key, out var root, out var index) || !current.numbers.TryGetValue(root, out var number))
			{
				return null;
			}
			return $"{Prefix}{number}/{index}";
		}

		/// <summary>The engine's key for the Obj a saved key names, or null for a copy that was not made again.</summary>
		public static string? ForLookup(string key)
		{
			return key.StartsWith(Prefix, StringComparison.Ordinal) ? Lookup(key, current?.made) : key;
		}

		/// <summary>The engine's key for a numbered copy's Obj, given the copies made so far.</summary>
		public static string? Lookup(string key, IReadOnlyList<Obj>? made)
		{
			if (made == null || !Split(key, out var number, out var index) || number >= (ulong)made.Count || !made[(int)number].IsValid)
			{
				return null;
			}
			return $"{Prefix}{made[(int)number].Id}/{index}";
		}

		/// <summary>The copy's number in the save, or -1 for a key that names no copy.</summary>
		public static int Number(string? key)
		{
			return key != null && key.StartsWith(Prefix, StringComparison.Ordinal) && Split(key, out var number, out _) ? (int)number : -1;
		}

		private static bool Split(string key, out ulong copy, out uint index)
		{
			var rest = key.AsSpan(Prefix.Length);
			var slash = rest.IndexOf('/');
			index = 0;
			copy = 0;
			return slash > 0
				&& ulong.TryParse(rest[..slash], NumberStyles.None, CultureInfo.InvariantCulture, out copy)
				&& uint.TryParse(rest[(slash + 1)..], NumberStyles.None, CultureInfo.InvariantCulture, out index);
		}
	}
}
