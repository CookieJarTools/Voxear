using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using Voxear.Interop;

namespace Voxear.Saving
{
	/// <summary>
	/// Saves kept in memory, away from the engine, as in the tests: the same slots, matched ignoring case, and the same
	/// listing, but nothing reaches a disk.
	/// </summary>
	internal sealed class MemorySaveStore : SaveStore
	{
		private readonly Dictionary<string, (string Name, long Time, byte[] Info, byte[] Data)> slots =
			new Dictionary<string, (string Name, long Time, byte[] Info, byte[] Data)>(StringComparer.OrdinalIgnoreCase);
		private long lastTime;

		public override string Folder => string.Empty;

		public override string DefaultFormat => SaveFormat.Json.Name;

		public override int Write(string slot, byte[] info, byte[] data)
		{
			Check(slot);
			var name = slots.TryGetValue(slot, out var existing) ? existing.Name : slot;
			lastTime = Math.Max(lastTime + 1, DateTimeOffset.UtcNow.ToUnixTimeMilliseconds());
			slots[slot] = (name, lastTime, info, data);
			return 0;
		}

		public override bool Wait(int ticket, bool block)
		{
			return true;
		}

		public override byte[] Read(string slot, int part)
		{
			var found = Find(slot);
			return part == NativeSaves.PartInfo ? found.Info : found.Data;
		}

		public override bool Delete(string slot)
		{
			Check(slot);
			return slots.Remove(slot);
		}

		public override void Copy(string from, string to)
		{
			var found = Find(from);
			Write(to, found.Info, found.Data);
		}

		public override void Rename(string from, string to)
		{
			var found = Find(from);
			Check(to);
			slots.Remove(from);
			slots[to] = (to, found.Time, found.Info, found.Data);
		}

		public override bool Exists(string slot)
		{
			Check(slot);
			return slots.ContainsKey(slot);
		}

		public override string Slots()
		{
			var buffer = new System.Buffers.ArrayBufferWriter<byte>();
			using (var json = new Utf8JsonWriter(buffer))
			{
				json.WriteStartArray();
				foreach (var slot in slots.Values.OrderByDescending(slot => slot.Time))
				{
					json.WriteStartObject();
					json.WriteString("name", slot.Name);
					json.WriteNumber("time", slot.Time);
					json.WriteNumber("size", slot.Info.Length + slot.Data.Length);
					json.WriteBoolean("picture", false);
					json.WritePropertyName("info");
					json.WriteRawValue(slot.Info);
					json.WriteEndObject();
				}
				json.WriteEndArray();
			}
			return System.Text.Encoding.UTF8.GetString(buffer.WrittenSpan);
		}

		private (string Name, long Time, byte[] Info, byte[] Data) Find(string slot)
		{
			Check(slot);
			return slots.TryGetValue(slot, out var found) ? found : throw new SaveException(SaveError.Missing, $"There is no save called \"{slot}\".");
		}

		private static void Check(string slot)
		{
			if (string.IsNullOrWhiteSpace(slot) || slot.IndexOfAny(new[] { '/', '\\', ':' }) >= 0)
			{
				throw new SaveException(SaveError.BadName, $"\"{slot}\" cannot name a save: use letters, digits, spaces, - and _.");
			}
		}
	}
}
