namespace Voxear.Tests
{
	/// <summary>Marked as a component, but it holds a string, which the engine's chunks cannot keep.</summary>
	internal struct TestHolder : IComponent
	{
		public string Label;
	}
}
