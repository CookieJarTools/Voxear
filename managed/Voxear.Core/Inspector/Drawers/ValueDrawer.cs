namespace Voxear;

/// <summary>
/// Draws every field of type T in the Inspector, and each item of a list of them, in place of the field's own
/// control: <c>class HealthDrawer : ValueDrawer&lt;Health&gt;</c>, whose Build adds the elements, with handlers that
/// read and set Value. A NextField among them keeps the field's own control inside the drawer.
/// <code>
/// public sealed class HealthDrawer : ValueDrawer&lt;Health&gt;
/// {
///     protected override void Build(UiElement root)
///     {
///         root.Add(new Label().Bind(() => $"{Value.Current} / {Value.Max}"));
///         root.Add(new Button("Fill", () => Value = Value with { Current = Value.Max }));
///     }
/// }
/// </code>
/// </summary>
public abstract class ValueDrawer<T> : Drawer<T>
{
}
