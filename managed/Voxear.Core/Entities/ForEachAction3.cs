namespace Voxear
{
	/// <summary>What EntityQuery.ForEach runs for each entity, with its components by reference to change in place.</summary>
	public delegate void ForEachAction<T1, T2, T3>(ref T1 first, ref T2 second, ref T3 third);
}
