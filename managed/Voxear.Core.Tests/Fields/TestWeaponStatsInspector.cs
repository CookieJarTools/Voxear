namespace Voxear.Tests;

internal sealed class TestWeaponStatsInspector : DefinitionInspector<TestWeaponStats>
{
	protected override void Build(UiElement root)
	{
		root.Add(new Label().Bind(() => $"{Target.Damage} damage"));
		root.Add(new FieldRow(nameof(TestWeaponStats.Shots)));
		root.Add(new NextField());
	}
}
