using System;
using System.Collections.Generic;
using System.Text.Json;
using Voxear.Saving;

namespace Voxear
{
	/// <summary>A save as a list of them shows it, for a load menu: its name, when it was written, and what the game said about it.</summary>
	public sealed class SaveSlot
	{
		private SaveSlot(string name, DateTime time, long size, SaveInfo info)
		{
			Name = name;
			Time = time;
			Size = size;
			Format = info.Format;
			Version = info.Version;
			Summary = info.Summary;
		}

		/// <summary>The name it was saved under.</summary>
		public string Name { get; }

		/// <summary>When it was written, in this computer's time zone.</summary>
		public DateTime Time { get; }

		/// <summary>Its file's size, in bytes.</summary>
		public long Size { get; }

		/// <summary>The format its data is in, such as "binary" or "json".</summary>
		public string Format { get; }

		/// <summary>The version of what it holds: the [SaveVersion] of the type saved, or what SaveFile.Version said.</summary>
		public int Version { get; }

		/// <summary>The words the game gave it when saving, such as "Chapter 2, 1h 20m", or null.</summary>
		public string? Summary { get; }

		public override string ToString()
		{
			return Summary == null ? $"{Name} ({Time:g})" : $"{Name}: {Summary} ({Time:g})";
		}

		/// <summary>The slots from the engine's listing.</summary>
		internal static List<SaveSlot> List(string json)
		{
			var slots = new List<SaveSlot>();
			using var document = JsonDocument.Parse(json);
			foreach (var slot in document.RootElement.EnumerateArray())
			{
				var time = DateTimeOffset.FromUnixTimeMilliseconds(slot.GetProperty("time").GetInt64()).LocalDateTime;
				var info = SaveInfo.From(slot.GetProperty("info"));
				slots.Add(new SaveSlot(slot.GetProperty("name").GetString()!, time, slot.GetProperty("size").GetInt64(), info));
			}
			return slots;
		}
	}
}
