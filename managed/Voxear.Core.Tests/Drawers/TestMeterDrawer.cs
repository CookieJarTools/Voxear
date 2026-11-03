using System.Collections.Generic;

namespace Voxear.Tests;

/// <summary>A drawer written only in Build: a label bound to the value, and a button and slider whose handlers set it.</summary>
internal sealed class TestMeterDrawer : ValueDrawer<TestMeter>
{
	public Label Text { get; private set; } = null!;

	public Button Fill { get; private set; } = null!;

	public Slider Bar { get; private set; } = null!;

	public List<int> Refreshed { get; } = new List<int>();

	protected override void Build(UiElement root)
	{
		Text = root.Add(new Label().Bind(() => $"{Value.Current} / {Value.Max}"));
		Fill = root.Add(new Button("Fill", () => Value = Value with { Current = Value.Max }));
		Bar = root.Add(new Slider(0f, 10f).OnMoved(moved => Value = Value with { Current = (int)moved }));
		root.Add(new NextField());
	}

	protected override void Refresh(TestMeter value)
	{
		Refreshed.Add(value.Current);
		Bar.Value = value.Current;
	}
}
