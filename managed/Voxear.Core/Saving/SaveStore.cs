using Voxear.Interop;

namespace Voxear.Saving;

/// <summary>
/// Where save files are kept: the engine's save folder, or, away from the engine, as in the tests, memory. Every
/// call throws a SaveException when it fails.
/// </summary>
internal abstract class SaveStore
{
	private static SaveStore? memory;

	public static SaveStore Current => Native.IsConnected ? NativeSaveStore.Instance : memory ??= new MemorySaveStore();

	public abstract string Folder { get; }

	/// <summary>The format the engine writes when scripts choose none: "json" or "binary".</summary>
	public abstract string DefaultFormat { get; }

	/// <summary>Queues a slot to be written, and returns a ticket for Wait.</summary>
	public abstract int Write(string slot, byte[] info, byte[] data);

	/// <summary>Whether a write has finished, waiting for it when block says so. Throws when it failed.</summary>
	public abstract bool Wait(int ticket, bool block);

	public abstract byte[] Read(string slot, int part);

	public abstract bool Delete(string slot);

	public abstract void Copy(string from, string to);

	public abstract void Rename(string from, string to);

	public abstract bool Exists(string slot);

	/// <summary>The slots as the engine lists them: JSON, newest first.</summary>
	public abstract string Slots();
}
