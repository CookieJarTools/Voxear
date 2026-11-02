using System;

namespace Voxear;

/// <summary>
/// Shows a field without its label, its control taking the whole row.
/// </summary>
[AttributeUsage(AttributeTargets.Field)]
public sealed class HideLabelAttribute : Attribute
{
}
