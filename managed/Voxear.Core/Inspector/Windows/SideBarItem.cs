namespace Voxear;

/// <summary>
/// Something of your own on the editor's side bar, below its buttons: built with Voxear UI as an EditorWindow is,
/// in Build or with Markup, and updated every frame in Update, while you edit and while the game plays. The side
/// bar is narrow, so keep it small: a light that says whether a server answers, a counter, a column of toggles.
/// <code>
/// public sealed class EnemyCount : SideBarItem
/// {
///     private Label count = null!;
///
///     protected override void Build(UiElement root)
///     {
///         count = root.Add(new Label());
///     }
///
///     protected override void Update(UiDocument ui)
///     {
///         count.Text = Obj.FindAll("Enemy").Length.ToString();
///     }
/// }
/// </code>
/// <para>For a button that runs a method, [SideBarButton] is all it takes.</para>
/// </summary>
public abstract class SideBarItem : EditorWindow
{
}
