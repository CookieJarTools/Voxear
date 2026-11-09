namespace Voxear;

/// <summary>
/// Where one of the fields goes inside a BehaviourInspector or a DefinitionInspector, as the Inspector shows it: its
/// label and control, and for a list its items, or for a [Serializable] value the fields under it. Name the field
/// as the script declares it, and a field inside a [Serializable] value after a dot:
/// <code>
/// protected override void Build(UiElement root)
/// {
///     root.Add(new Label("Shots"));
///     root.Add(new FieldRow(nameof(WeaponStats.Shots)));
///     root.Add(new FieldRow("Recoil.Kick"));
///     root.Add(new NextField());
/// }
/// </code>
/// <para>
/// A NextField holds the fields no FieldRow names. In markup, the same is <c>&lt;FieldRow field="Shots" /&gt;</c>.
/// Outside an inspector it is an empty panel.
/// </para>
/// </summary>
public sealed class FieldRow : Panel
{
	public FieldRow(string field) : base("field-row", $"field-row:{field}")
	{
		Field = field;
	}

	/// <summary>The field it holds, as the script declares it.</summary>
	public string Field { get; }
}
