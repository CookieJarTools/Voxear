using System.Numerics;

namespace Voxear;

/// <summary>The ground under a point: how high it is, which way it faces, and the Obj of the terrain it is on.</summary>
public readonly record struct TerrainGround(float Height, Vector3 Normal, Obj Terrain);
