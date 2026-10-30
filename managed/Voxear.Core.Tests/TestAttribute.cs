using System;

namespace Voxear.Tests
{
	/// <summary>Marks a static method the runner calls. It passes unless it throws.</summary>
	[AttributeUsage(AttributeTargets.Method)]
	internal sealed class TestAttribute : Attribute
	{
	}
}
