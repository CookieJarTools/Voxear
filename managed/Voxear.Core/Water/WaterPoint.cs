using System.Numerics;

namespace Voxear;

/// <summary>The water at a point, as <see cref="Water.TrySample"/> finds it.</summary>
/// <param name="Height">How high its surface is, in the world.</param>
/// <param name="Normal">The way its surface faces.</param>
/// <param name="Flow">How fast it flows, in metres a second.</param>
public readonly record struct WaterPoint(float Height, Vector3 Normal, Vector3 Flow);
