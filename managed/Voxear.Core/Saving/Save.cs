using System;
using System.Collections.Generic;
using System.Diagnostics.CodeAnalysis;
using System.Runtime.Loader;
using Voxear.Saving;
using Voxear.Scripting;

namespace Voxear
{
	/// <summary>
	/// The game's saves, each in a slot of its own name: Save.Write("slot1", data) and Save.Read&lt;GameData&gt;("slot1"),
	/// with nothing to set up. Any value saves: numbers, text, enums, vectors and colours, lists and dictionaries,
	/// classes and structs by their fields and properties, Objs and assets by what finds them again, and a behaviour by
	/// its fields. Each save is written whole beside the last one and then put in its place, so a crash halfway never
	/// costs the save before. SaveFile saves several parts in one slot, with a summary for a load menu.
	/// </summary>
	public static class Save
	{
		private static SaveFormat? format;

		/// <summary>
		/// The format saves are written in when a SaveFile names none: JSON in the editor and with vox run, to read and
		/// change by hand, and binary in builds, which is smaller and quicker. Reading always uses the format a save
		/// was written in.
		/// </summary>
		public static SaveFormat Format
		{
			get { return format ?? (SaveStore.Current.DefaultFormat == SaveFormat.Json.Name ? SaveFormat.Json : SaveFormat.Binary); }
			set
			{
				format = value;
				if (value != null)
				{
					SaveTypes.Add(value);
				}
			}
		}

		/// <summary>The folder saves are kept in, in the player's own data folder, or empty when there is none.</summary>
		public static string Folder => SaveStore.Current.Folder;

		/// <summary>
		/// Every save, newest first. It reads the start of each save's file, not the whole save, so it is quick enough
		/// for a menu; keep the list while the menu is open rather than asking each frame.
		/// </summary>
		public static IReadOnlyList<SaveSlot> Slots => SaveSlot.List(SaveStore.Current.Slots());

		/// <summary>
		/// Saves data in a slot, replacing what it held. The game waits while the save reaches the disk, which can be
		/// long enough to see as a hitch on a slow disk; WriteAsync does not wait. Throws a SaveException when it cannot.
		/// </summary>
		public static void Write<T>(string slot, T data, string? summary = null)
		{
			SaveFile.Of(data, slot, summary).Write(slot);
		}

		/// <summary>
		/// Saves data in a slot while the game carries on: the values are taken apart now, so they can change straight
		/// away, and the disk is written on another thread. Await it to know the save is safely written.
		/// </summary>
		public static SaveWrite WriteAsync<T>(string slot, T data, string? summary = null)
		{
			return SaveFile.Of(data, slot, summary).WriteAsync(slot);
		}

		/// <summary>
		/// Reads a slot as a T. Throws a SaveException when there is no such slot, when its file is damaged, or when a
		/// newer version of the game wrote it.
		/// </summary>
		public static T Read<T>(string slot)
		{
			return SaveFile.Read(slot).ReadAll<T>();
		}

		/// <summary>Reads a slot as a T, or gives fallback when there is none. A save that cannot be read gives fallback too, saying why in the log.</summary>
		public static T Read<T>(string slot, T fallback)
		{
			return TryRead<T>(slot, out var data) ? data : fallback;
		}

		/// <summary>Reads a slot as a T, or gives false when there is none or it cannot be read, saying why in the log.</summary>
		public static bool TryRead<T>(string slot, [MaybeNullWhen(false)] out T data)
		{
			try
			{
				data = Read<T>(slot);
				return true;
			}
			catch (SaveException exception)
			{
				if (exception.Error != SaveError.Missing)
				{
					Log.Warning(exception.Message);
				}
				data = default;
				return false;
			}
		}

		/// <summary>
		/// Fills an object that already exists from a slot, such as a behaviour, which a save cannot make: Save.Write
		/// keeps a behaviour's fields, and this puts them back. Throws a SaveException as Read does.
		/// </summary>
		public static void ReadInto(string slot, object target)
		{
			SaveFile.Read(slot).ReadAllInto(target);
		}

		public static bool Exists(string slot)
		{
			return SaveStore.Current.Exists(slot);
		}

		/// <summary>Deletes a slot. False when there was none.</summary>
		public static bool Delete(string slot)
		{
			return SaveStore.Current.Delete(slot);
		}

		/// <summary>Copies a slot to another name, replacing what that held, as for a backup before a risky choice.</summary>
		public static void Copy(string from, string to)
		{
			SaveStore.Current.Copy(from, to);
		}

		/// <summary>Gives a slot another name, replacing what that held.</summary>
		public static void Rename(string from, string to)
		{
			SaveStore.Current.Rename(from, to);
		}

		/// <summary>
		/// Saves T with this converter, in place of any the game found by itself. A converter class with a constructor
		/// that takes nothing needs no adding: the game finds it. Ones added here are dropped when the scripts reload.
		/// </summary>
		public static void AddConverter<T>(SaveConverter<T> converter)
		{
			SaveTypes.Add(converter);
		}

		/// <summary>Lets saves be read in a format the game does not find by itself, such as one that needs settings.</summary>
		public static void AddFormat(SaveFormat format)
		{
			SaveTypes.Add(format);
		}

		/// <summary>
		/// Changes T's saved values from version from to the next, as a [MigrateFrom(from)] method on T does, for a
		/// type whose code you cannot change.
		/// </summary>
		public static void AddMigration<T>(int from, Action<SaveNode> migrate)
		{
			SaveTypes.AddMigration(typeof(T), from, migrate);
		}

		/// <summary>Lets go of a format from the old scripts after a reload.</summary>
		internal static void Forget(AssemblyLoadContext old)
		{
			if (format != null && GameCode.Holds(format, old))
			{
				format = null;
			}
		}
	}
}
