using Voxear.Interop;

namespace Voxear;

/// <summary>A prefab loading in the background, from Prefab.LoadAsync. Awaiting it gives the prefab.</summary>
public sealed class PrefabLoad : AsyncLoad
{
	internal PrefabLoad(int id) : base(id)
	{
	}

	/// <summary>The loaded prefab, or None until the load is done (and if it fails).</summary>
	public Prefab Prefab { get; private set; }

	public PrefabLoadAwaiter GetAwaiter()
	{
		return new PrefabLoadAwaiter(this);
	}

	internal override void Read(LoadProgress status)
	{
		if (status.Prefab >= 0)
		{
			Prefab = Prefab.FromEngine(status.Prefab);
		}
	}
}
