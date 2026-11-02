namespace Voxear
{
	/// <summary>
	/// Draws a whole behaviour in the Inspector, in place of its fields, as Unity's custom Editor does:
	/// <c>class TowerInspector : BehaviourInspector&lt;Tower&gt;</c>. It draws behaviours of type T and of types derived
	/// from it, unless they have an inspector of their own. Build its elements in code, in Build, or describe them in
	/// Markup, whose element named "next" is where the behaviour's fields show as they would without it.
	/// <para>
	/// Draw gets a copy of the behaviour made from the scene's values, on its Obj, and whatever it changes in the
	/// copy's fields is saved in the scene, as one step to undo. It runs in edit mode; while the game plays, the
	/// behaviour shows its fields.
	/// </para>
	/// <code>
	/// protected override void Draw(Tower tower, UiDocument ui)
	/// {
	///     if (raise.WasClicked)
	///     {
	///         tower.Floors++;
	///     }
	///     floors.Text = $"{tower.Floors} floors";
	/// }
	/// </code>
	/// </summary>
	public abstract class BehaviourInspector<T> : Drawer
		where T : Behaviour
	{
		/// <summary>
		/// Shows the behaviour in the elements, and changes its fields as what was clicked or changed in them asks. It is
		/// called when the Inspector first shows the behaviour, when its values change elsewhere, as on undo, and after
		/// something in it was clicked or changed, so an element's WasClicked says whether that was it.
		/// </summary>
		protected abstract void Draw(T behaviour, UiDocument ui);

		internal sealed override object? DrawValue(object? value, UiDocument ui)
		{
			Draw((T)value!, ui);
			return value;
		}
	}
}
