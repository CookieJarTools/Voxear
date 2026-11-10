using System;

namespace Voxear;

/// <summary>
/// Marks a behaviour's field keeping a position in the world, a Vector3 or an array or list of them, such as where
/// an enemy last saw the player or the points of a patrol: when the world's middle moves (see Origin), the field
/// moves with everything else. On a [Replicated] Vector3 property or a Vector3 RPC argument, it is sent measured
/// from the level's middle, so players whose worlds' middles have moved apart agree on where it is.
/// <code>
/// [WorldPosition] Vector3 lastSeen;
/// [WorldPosition] List&lt;Vector3&gt; patrol = [];
/// [Replicated, WorldPosition] public partial Vector3 Rally { get; set; }
/// [ServerRpc] void Signal([WorldPosition] Vector3 at) { }
/// </code>
/// </summary>
[AttributeUsage(AttributeTargets.Field | AttributeTargets.Property | AttributeTargets.Parameter)]
public sealed class WorldPositionAttribute : Attribute
{
}
