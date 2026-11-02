namespace Voxear.Tests;

/// <summary>Draws whatever is tagged, leaving it as it is, and notes that it drew.</summary>
internal sealed class TestTagDrawer : AttributeDrawer<TestTagAttribute>
{
	public override string Markup => "<vui><Panel name=\"next\" /></vui>";

	protected override object? Draw(object? value, UiDocument ui)
	{
		TestHealthDrawer.Drawn.Add("tag");
		return value;
	}
}
