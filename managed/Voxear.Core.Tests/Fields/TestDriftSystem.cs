namespace Voxear.Tests;

/// <summary>A system, as ScriptInfo lists them for the editor's Entities panel.</summary>
[UpdateIn(Stage.FixedUpdate)]
[ExecutionOrder(5)]
internal sealed class TestDriftSystem : SystemBase
{
	protected override void Update()
	{
	}
}
