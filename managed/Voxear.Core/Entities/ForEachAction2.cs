namespace Voxear;

/// <summary>What EntityQuery.ForEach runs for each entity, with its components by reference to change in place.</summary>
public delegate void ForEachAction<T1, T2>(ref T1 first, ref T2 second);
