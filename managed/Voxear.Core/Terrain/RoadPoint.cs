using System.Numerics;

namespace Voxear;

/// <summary>A point of a Road, in its Obj's own space, and how wide the road is there, in blocks.</summary>
public readonly record struct RoadPoint(Vector3 At, float Width);
