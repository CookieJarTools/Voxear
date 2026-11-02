using System;

namespace Voxear
{
	/// <summary>
	/// Shows an icon in the Scene view where each Obj with this behaviour is, as lights and cameras show theirs, so an
	/// Obj with nothing to see can still be found and clicked: <c>[GizmoIcon(IconName.Audio)]</c>. The Gizmos menu
	/// hides it with the type's gizmos.
	/// </summary>
	[AttributeUsage(AttributeTargets.Class, Inherited = true)]
	public sealed class GizmoIconAttribute : Attribute
	{
		public GizmoIconAttribute(IconName icon)
		{
			Icon = icon;
		}

		public IconName Icon { get; }
	}
}
