using System;

namespace Voxear;

/// <summary>
/// Keeps a field the scene can set out of the Inspector.
/// </summary>
[AttributeUsage(AttributeTargets.Field)]
public sealed class HideInInspectorAttribute : Attribute
{
}
