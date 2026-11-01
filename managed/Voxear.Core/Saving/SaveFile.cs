using System;
using System.Diagnostics.CodeAnalysis;
using Voxear.Interop;
using Voxear.Saving;
using Voxear.Scripting;

namespace Voxear
{
	/// <summary>
	/// One save with several parts, written together in one go: the player, the world, the quests, each set by its own
	/// name and read back by it, so each part of a game can save its own values without one class holding them all.
	/// It also chooses its own format, summary and version. Save.Write is a SaveFile with one value at the top.
	/// </summary>
	public sealed class SaveFile
	{
		/// <summary>The slot it was read from, for messages.</summary>
		private readonly string context;

		public SaveFile() : this(SaveNode.NewTable(), null, string.Empty)
		{
		}

		private SaveFile(SaveNode root, string? type, string context)
		{
			Root = root;
			TypeName = type;
			this.context = context;
		}

		/// <summary>Everything the save holds. Its parts are the values of this table.</summary>
		public SaveNode Root { get; }

		/// <summary>Words a load menu shows for the save, such as "Chapter 2, 1h 20m". Save.Slots reads them without reading the save.</summary>
		public string? Summary { get; set; }

		/// <summary>The format to write in, or null for Save.Format. A save read back says the format it was in.</summary>
		public SaveFormat? Format { get; set; }

		/// <summary>A version of the game's own for the save, which Save.Slots shows. Each type's [SaveVersion] is kept with its values, apart from this.</summary>
		public int Version { get; set; } = 1;

		private string? TypeName { get; }

		/// <summary>Sets a part, taking its value apart now: changing the value afterwards changes nothing in the save.</summary>
		public void Set<T>(string part, T value)
		{
			Root[part] = SaveWriter.Write(value, typeof(T), part);
		}

		/// <summary>Reads a part as a T. Throws a SaveException when the save has no such part.</summary>
		public T Get<T>(string part)
		{
			return Root.Has(part) ? Bind<T>(Root[part], part) : throw MissingPart(part);
		}

		/// <summary>Reads a part as a T, or gives fallback when the save has no such part.</summary>
		public T Get<T>(string part, T fallback)
		{
			return Root.Has(part) ? Bind<T>(Root[part], part) : fallback;
		}

		public bool Has(string part)
		{
			return Root.Has(part);
		}

		public bool Remove(string part)
		{
			return Root.Remove(part);
		}

		/// <summary>Fills an object that already exists from a part, such as a behaviour from what Set saved of it.</summary>
		public void ReadInto(string part, object target)
		{
			if (!Root.Has(part))
			{
				throw MissingPart(part);
			}
			Fill(Root[part], target, part);
		}

		/// <summary>Writes the save, replacing what the slot held only once all of it is on disk. Throws a SaveException when it cannot.</summary>
		public void Write(string slot)
		{
			var (info, data) = Encode();
			var store = SaveStore.Current;
			store.Wait(store.Write(slot, info, data), true);
		}

		/// <summary>
		/// Writes the save while the game carries on. Its values are taken apart into bytes now, so the game can change
		/// them straight away; only the disk is left to wait for.
		/// </summary>
		public SaveWrite WriteAsync(string slot)
		{
			var (info, data) = Encode();
			return SaveWrite.Start(slot, info, data);
		}

		/// <summary>Reads a slot. Throws a SaveException when there is none, or it cannot be read.</summary>
		public static SaveFile Read(string slot)
		{
			var store = SaveStore.Current;
			var info = SaveInfo.Parse(store.Read(slot, NativeSaves.PartInfo), slot);
			var format = SaveTypes.Format(info.Format)
				?? throw new SaveException(SaveError.Unsupported, $"\"{slot}\" is in a format called {info.Format}, which this game does not have.");
			var data = store.Read(slot, NativeSaves.PartData);
			SaveNode root;
			try
			{
				root = format.Read(data);
			}
			catch (FormatException exception)
			{
				throw new SaveException(SaveError.Damaged, $"The save \"{slot}\" cannot be read: {exception.Message}", exception);
			}
			return new SaveFile(root, info.Type, slot) { Summary = info.Summary, Format = format, Version = info.Version };
		}

		/// <summary>Reads a slot, or gives false when there is none or it cannot be read, saying why in the log.</summary>
		public static bool TryRead(string slot, [NotNullWhen(true)] out SaveFile? file)
		{
			try
			{
				file = Read(slot);
				return true;
			}
			catch (SaveException exception)
			{
				if (exception.Error != SaveError.Missing)
				{
					Log.Warning(exception.Message);
				}
				file = null;
				return false;
			}
		}

		/// <summary>A save of one value, at the top rather than as a part.</summary>
		internal static SaveFile Of<T>(T value, string slot, string? summary)
		{
			var type = value?.GetType() ?? typeof(T);
			var root = SaveWriter.Write(value, typeof(T), slot);
			return new SaveFile(root, value == null ? null : SaveTypes.NameOf(type), slot) { Summary = summary, Version = SaveTypes.Version(type).Version };
		}

		/// <summary>Reads the value at the top, as Save.Read does.</summary>
		internal T ReadAll<T>()
		{
			return Bind<T>(Root, context);
		}

		/// <summary>Fills an object from the value at the top, as Save.ReadInto does.</summary>
		internal void ReadAllInto(object target)
		{
			Fill(Root, target, context);
		}

		private (byte[] Info, byte[] Data) Encode()
		{
			var format = Format ?? Save.Format;
			var info = new SaveInfo(format.Name, TypeName, Version, Summary);
			return (info.ToBytes(), format.Write(Root));
		}

		private static T Bind<T>(SaveNode node, string where)
		{
			try
			{
				return (T)SaveReader.Read(node, typeof(T), null, where)!;
			}
			catch (Exception exception) when (exception is not SaveException && BehaviourFields.IsBadValue(exception))
			{
				throw new SaveException(SaveError.Unsupported, $"Reading \"{where}\": {exception.Message}", exception);
			}
		}

		private static void Fill(SaveNode node, object target, string where)
		{
			try
			{
				SaveReader.Read(node, target.GetType(), target, where);
			}
			catch (Exception exception) when (exception is not SaveException && BehaviourFields.IsBadValue(exception))
			{
				throw new SaveException(SaveError.Unsupported, $"Reading \"{where}\" into {target.GetType().Name}: {exception.Message}", exception);
			}
		}

		private SaveException MissingPart(string part)
		{
			var about = context.Length > 0 ? $"The save \"{context}\"" : "The save";
			return new SaveException(SaveError.Missing, $"{about} has no part called \"{part}\".");
		}
	}
}
