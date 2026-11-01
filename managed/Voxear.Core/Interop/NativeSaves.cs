using System.Text;

namespace Voxear.Interop
{
	/// <summary>Thin wrappers over the engine's save calls. A failure comes back as a SaveException with the engine's message.</summary>
	internal static unsafe class NativeSaves
	{
		public const int PartInfo = 0;
		public const int PartData = 2;
		public const int ChangeDelete = 0;
		public const int ChangeCopy = 1;
		public const int ChangeRename = 2;
		public const int ChangeExists = 3;
		public const int InfoSlots = 0;
		public const int InfoFolder = 1;
		public const int InfoFormat = 2;
		private const int InfoError = 3;

		/// <summary>Queues a slot to be written and returns its ticket.</summary>
		public static int Write(string slot, byte[] info, byte[] data)
		{
			var name = Encoding.UTF8.GetBytes(slot);
			fixed (byte* namePtr = name)
			fixed (byte* infoPtr = info)
			fixed (byte* dataPtr = data)
			{
				return Check(Native.Api->SaveWrite(namePtr, name.Length, infoPtr, info.Length, dataPtr, data.LongLength));
			}
		}

		/// <summary>Whether a write has finished, waiting for it when asked.</summary>
		public static bool Wait(int ticket, bool block)
		{
			return Check(Native.Api->SaveWait(ticket, block ? 1 : 0)) == 1;
		}

		public static byte[] Read(string slot, int part)
		{
			var name = Encoding.UTF8.GetBytes(slot);
			fixed (byte* namePtr = name)
			{
				var length = Native.Api->SaveRead(namePtr, name.Length, part, null, 0);
				Check((int)System.Math.Min(length, 0));
				var bytes = new byte[length];
				fixed (byte* buffer = bytes)
				{
					Check((int)System.Math.Min(Native.Api->SaveRead(namePtr, name.Length, part, buffer, length), 0));
				}
				return bytes;
			}
		}

		/// <summary>Deletes, copies, renames or looks for a slot: 1 for done or found, 0 for nothing there.</summary>
		public static int Change(int change, string slot, string other)
		{
			var name = Encoding.UTF8.GetBytes(slot);
			var second = Encoding.UTF8.GetBytes(other);
			fixed (byte* namePtr = name)
			fixed (byte* otherPtr = second)
			{
				return Check(Native.Api->SaveChange(change, namePtr, name.Length, otherPtr, second.Length));
			}
		}

		public static string Info(int what)
		{
			var length = Check(Native.Api->SaveInfo(what, null, 0));
			var bytes = new byte[length];
			fixed (byte* buffer = bytes)
			{
				Native.Api->SaveInfo(what, buffer, length);
			}
			return Encoding.UTF8.GetString(bytes);
		}

		/// <summary>
		/// What a save calls something, so a later run finds it: an Obj (kind 0), an asset by project path or GUID
		/// (kind 1), or a loaded prefab (kind 2). Null for an Obj made while the game ran, or away from the engine.
		/// </summary>
		public static string? Key(int kind, ulong id, string reference)
		{
			if (!Native.IsConnected)
			{
				return null;
			}

			var text = Encoding.UTF8.GetBytes(reference);
			fixed (byte* textPtr = text)
			{
				var length = Native.Api->SaveKey(kind, id, textPtr, text.Length, null, 0);
				if (length < 0)
				{
					return null;
				}

				var bytes = new byte[length];
				fixed (byte* buffer = bytes)
				{
					Native.Api->SaveKey(kind, id, textPtr, text.Length, buffer, length);
				}
				return Encoding.UTF8.GetString(bytes);
			}
		}

		/// <summary>The Obj a key names, or None when its scene is not open.</summary>
		public static Obj FindObj(string key)
		{
			if (!Native.IsConnected)
			{
				return Obj.None;
			}

			var text = Encoding.UTF8.GetBytes(key);
			fixed (byte* textPtr = text)
			{
				return new Obj(Native.Api->SaveObj(textPtr, text.Length));
			}
		}

		/// <summary>The result when it is not a failure; a failure, below 0, thrown with the engine's message.</summary>
		private static int Check(int result)
		{
			if (result >= 0)
			{
				return result;
			}

			var length = Native.Api->SaveInfo(InfoError, null, 0);
			var bytes = new byte[System.Math.Max(length, 0)];
			fixed (byte* buffer = bytes)
			{
				Native.Api->SaveInfo(InfoError, buffer, bytes.Length);
			}
			var message = Encoding.UTF8.GetString(bytes);
			throw new SaveException((SaveError)(-result), message.Length > 0 ? Capitalized(message) + "." : "Saving failed.");
		}

		private static string Capitalized(string text)
		{
			return char.ToUpperInvariant(text[0]) + text[1..];
		}
	}
}
