using System;

namespace Voxear;

/// <summary>
/// Leaves a field or property out of saves, such as one worked out again from the others. [NonSerialized] leaves a
/// field out of saves too, and out of scenes as well.
/// </summary>
[AttributeUsage(AttributeTargets.Field | AttributeTargets.Property)]
public sealed class DontSaveAttribute : Attribute
{
}
