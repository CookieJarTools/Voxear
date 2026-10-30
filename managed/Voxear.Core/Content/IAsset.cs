namespace Voxear
{
	/// <summary>An asset type that Ref&lt;T&gt; can load. Prefab is the first; textures, materials and sounds follow.</summary>
	public interface IAsset<TSelf> where TSelf : struct, IAsset<TSelf>
	{
		/// <summary>Loads the asset by project path or GUID. Logs an error and returns an invalid one when it cannot.</summary>
		static abstract TSelf Load(string path);
	}
}
