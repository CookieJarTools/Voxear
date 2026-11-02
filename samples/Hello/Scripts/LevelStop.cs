using System;
using Voxear;

namespace Hello;

/// <summary>One step of the GameManager's cycle: a level to load once this many seconds have passed.</summary>
[Serializable]
public struct LevelStop
{
	public LevelAsset Level;
	public float Seconds;
}
