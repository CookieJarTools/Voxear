using System;

namespace Voxear;

/// <summary>
/// Runs this behaviour type earlier (lower values) or later than others within every lifecycle phase.
/// Behaviours with the same order run in creation order, which is hierarchy order for a loaded scene. On a
/// system, it orders the systems of its stage, and systems with the same order run in name order.
/// </summary>
[AttributeUsage(AttributeTargets.Class, Inherited = true)]
public sealed class ExecutionOrderAttribute : Attribute
{
	public ExecutionOrderAttribute(int order)
	{
		Order = order;
	}

	public int Order { get; }
}
