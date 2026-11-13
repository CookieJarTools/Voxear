using System.Numerics;

namespace Voxear;

/// <summary>A sapling growing: where it stands, in metres, and how far grown it is into its tree, from 0 to 1.</summary>
public readonly record struct SaplingInfo(Vector3 Position, float Grown);
