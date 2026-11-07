using System.Numerics;

namespace Voxear;

/// <summary>A shot's line: where it starts, at the eye, and the way it goes, spread as loosely as the gun was held.</summary>
/// <param name="Origin">Where the shot starts, in the world.</param>
/// <param name="Direction">The way it goes, a unit vector.</param>
public readonly record struct Shot(Vector3 Origin, Vector3 Direction);
