using Voxear.Interop;

namespace Voxear.Saving
{
	/// <summary>The engine's save folder, where the engine writes each save to a file of its own.</summary>
	internal sealed class NativeSaveStore : SaveStore
	{
		public static readonly NativeSaveStore Instance = new NativeSaveStore();

		public override string Folder => NativeSaves.Info(NativeSaves.InfoFolder);

		public override string DefaultFormat => NativeSaves.Info(NativeSaves.InfoFormat);

		public override int Write(string slot, byte[] info, byte[] data)
		{
			return NativeSaves.Write(slot, info, data);
		}

		public override bool Wait(int ticket, bool block)
		{
			return NativeSaves.Wait(ticket, block);
		}

		public override byte[] Read(string slot, int part)
		{
			return NativeSaves.Read(slot, part);
		}

		public override bool Delete(string slot)
		{
			return NativeSaves.Change(NativeSaves.ChangeDelete, slot, string.Empty) == 1;
		}

		public override void Copy(string from, string to)
		{
			NativeSaves.Change(NativeSaves.ChangeCopy, from, to);
		}

		public override void Rename(string from, string to)
		{
			NativeSaves.Change(NativeSaves.ChangeRename, from, to);
		}

		public override bool Exists(string slot)
		{
			return NativeSaves.Change(NativeSaves.ChangeExists, slot, string.Empty) == 1;
		}

		public override string Slots()
		{
			return NativeSaves.Info(NativeSaves.InfoSlots);
		}
	}
}
