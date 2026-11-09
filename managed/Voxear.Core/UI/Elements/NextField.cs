namespace Voxear;

/// <summary>
/// Where whatever would show without your drawer goes, inside it: the field's next drawer, the field's usual
/// control, or for a BehaviourInspector or a DefinitionInspector the usual fields no FieldRow holds, as an element
/// named "next" does in markup.
/// It is what Unity's PropertyField and DrawDefaultInspector give, as a place rather than a call:
/// <code>
/// protected override void Build(UiElement root)
/// {
///     root.Add(new Button("-", () => Value -= Attribute.Step));
///     root.Add(new NextField());
///     root.Add(new Button("+", () => Value += Attribute.Step));
/// }
/// </code>
/// <para>
/// With one, a drawer wraps what was there; without one, it replaces it. Keep it once it is added: what goes in it
/// goes with it. Outside a drawer it is an empty panel.
/// </para>
/// </summary>
public sealed class NextField : Panel
{
	public NextField() : base("field-drawer-next")
	{
	}
}
