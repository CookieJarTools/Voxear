using System.Numerics;

namespace Voxear;

/// <summary>A box of a Ground Fit's footprint: its centre and size along x and z, in metres in its Obj's own space.</summary>
public readonly record struct FitBox(Vector2 Centre, Vector2 Size);
