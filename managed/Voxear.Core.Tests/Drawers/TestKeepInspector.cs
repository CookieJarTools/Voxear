namespace Voxear.Tests;

/// <summary>An inspector written only in Build, whose button changes the behaviour through Target.</summary>
internal sealed class TestKeepInspector : BehaviourInspector<TestKeep>
{
	public static TestKeepInspector? Last { get; private set; }

	public Label Summary { get; private set; } = null!;

	public Button Raise { get; private set; } = null!;

	public int Refreshes { get; private set; }

	protected override void Build(UiElement root)
	{
		Last = this;
		Summary = root.Add(new Label().Bind(() => $"{Target.Floors} floors"));
		Raise = root.Add(new Button("Add a floor", () => Target.Floors++));
		root.Add(new NextField());
	}

	protected override void Refresh(TestKeep behaviour)
	{
		Refreshes++;
	}
}
