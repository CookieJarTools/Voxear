using System;

namespace Voxear;

/// <summary>
/// Marks a behaviour's field keeping a position in the world, a Vector3 or an array or list of them, such as where
/// an enemy last saw the player or the points of a patrol: when the world's middle moves (see Origin), the field
/// moves with everything else.
/// <code>
/// [WorldPosition] Vector3 lastSeen;
/// [WorldPosition] List&lt;Vector3&gt; patrol = [];
/// </code>
/// </summary>
[AttributeUsage(AttributeTargets.Field)]
public sealed class WorldPositionAttribute : Attribute
{
}
