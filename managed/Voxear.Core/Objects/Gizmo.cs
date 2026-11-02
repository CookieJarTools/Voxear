namespace Voxear
{
	/// <summary>
	/// What a behaviour's DrawGizmos is told about its Obj in the Scene view: whether it is selected, and whether the
	/// pointer is over it, so a gizmo can show more when it matters and stay out of the way otherwise.
	/// </summary>
	public sealed class Gizmo
	{
		internal Gizmo(bool selected, bool hovered)
		{
			IsSelected = selected;
			IsHovered = hovered;
		}

		/// <summary>Whether the behaviour's Obj is the one selected.</summary>
		public bool IsSelected { get; }

		/// <summary>Whether the pointer is over the behaviour's Obj in the Scene view.</summary>
		public bool IsHovered { get; }
	}
}
