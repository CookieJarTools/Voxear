namespace Voxear
{
	/// <summary>
	/// Marks a behaviour as the one instance of its type, reachable from anywhere through Instance of T.
	/// The engine registers it before any Awake runs, warns about duplicates, and the editor shows a
	/// badge on the Obj that carries it.
	/// </summary>
	public interface IInstance
	{
	}
}
