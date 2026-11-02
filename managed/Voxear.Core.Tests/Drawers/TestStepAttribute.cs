using System;

namespace Voxear.Tests;

[AttributeUsage(AttributeTargets.Field)]
internal sealed class TestStepAttribute : Attribute
{
	public TestStepAttribute(int step)
	{
		Step = step;
	}

	public int Step { get; }
}
