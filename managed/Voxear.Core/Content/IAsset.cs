namespace Voxear;

/// <summary>An asset type that Ref&lt;T&gt; can load. Prefab and Sound so far; textures and materials follow.</summary>
public interface IAsset<TSelf> where TSelf : struct, IAsset<TSelf>
{
	/// <summary>Loads the asset by project path or GUID. Logs an error and returns an invalid one when it cannot.</summary>
	static abstract TSelf Load(string path);

	/// <summary>
	/// The asset's GUID, which a save keeps to load it again with Load: null for an invalid one, or away from the
	/// engine. Every asset type gives it, so saves keep each one without knowing it by name.
	/// </summary>
	internal string? AssetGuid { get; }
}
