namespace Voxear
{
	/// <summary>
	/// Marks an unmanaged struct as a component: plain data the engine keeps in its chunks beside its own components,
	/// so systems read and write it in place through spans. It is registered by its full name the first time it is
	/// used, and holds no references, which is what lets it live outside the garbage-collected heap.
	/// <code>public struct Spin : IComponent { public float Speed; }</code>
	/// </summary>
	public interface IComponent
	{
	}
}
