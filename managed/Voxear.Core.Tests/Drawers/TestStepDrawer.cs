namespace Voxear.Tests
{
	/// <summary>Adds its attribute's step each time it draws: it draws only whole numbers.</summary>
	internal sealed class TestStepDrawer : AttributeDrawer<TestStepAttribute, int>
	{
		public override string Markup => "<vui><Button name=\"step\">+</Button></vui>";

		protected override int Draw(int value, UiDocument ui)
		{
			return value + Attribute.Step;
		}
	}
}
