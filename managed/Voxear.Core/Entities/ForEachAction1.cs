namespace Voxear
{
	/// <summary>What EntityQuery.ForEach runs for each entity, with its component by reference to change in place.</summary>
	public delegate void ForEachAction<T1>(ref T1 first);
}
