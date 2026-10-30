using System;

namespace Voxear
{
	/// <summary>
	/// Lets scenes and prefabs set a private behaviour field, as Unity's SerializeField does. Public fields can be set
	/// without it; [NonSerialized] opts one out.
	/// </summary>
	[AttributeUsage(AttributeTargets.Field)]
	public sealed class SerializeFieldAttribute : Attribute
	{
	}
}
