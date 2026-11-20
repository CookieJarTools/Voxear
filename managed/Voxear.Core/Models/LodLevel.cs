using System.Collections.Generic;

namespace Voxear;

/// <summary>
/// One of a LOD Group's levels: the Objs it shows, each with everything inside it, and where it ends: by screen size,
/// the share of the screen's height, from 0 to 1, it shows down to; by distance, the metres it shows out to.
/// </summary>
public readonly record struct LodLevel(IReadOnlyList<Obj> Objs, float Until);
