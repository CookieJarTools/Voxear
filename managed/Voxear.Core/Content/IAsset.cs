namespace Voxear
{
	/// <summary>An asset type that Ref&lt;T&gt; can load. Prefab and Sound so far; textures and materials follow.</summary>
	public interface IAsset<TSelf> where TSelf : struct, IAsset<TSelf>
	{
		/// <summary>Loads the asset by project path or GUID. Logs an error and returns an invalid one when it cannot.</summary>
		static abstract TSelf Load(string path);
	}
}
