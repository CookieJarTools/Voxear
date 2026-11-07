using System;

namespace Voxear;

/// <summary>
/// Shares a behaviour's property with a networked game's players: set on the server, it changes on every player's
/// machine too. Mark a partial property of a behaviour on an Obj with a Net Object:
/// <code>
/// public sealed partial class Health : Behaviour
/// {
/// 	[Replicated]
/// 	public partial int Points { get; set; }
/// }
/// </code>
/// Voxear writes the property, so a change is noticed when it is made: bool, int, long, float, double, string,
/// Vector2, Vector3, Quaternion, enums, Obj and NetPlayer can be replicated. An Obj reads as None on a machine that
/// does not have it yet.
/// </summary>
[AttributeUsage(AttributeTargets.Property, Inherited = false)]
public sealed class ReplicatedAttribute : Attribute
{
}
