namespace Voxear
{
	/// <summary>
	/// A data asset described in script, like a ScriptableObject. Designers create and edit Definitions
	/// in the editor, and code holds references to them like any other asset.
	/// </summary>
	public abstract class Definition
	{
		public string Name { get; internal set; } = string.Empty;
	}
}
