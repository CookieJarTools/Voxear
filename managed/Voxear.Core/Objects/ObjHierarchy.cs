using System;
using System.Globalization;
using Voxear.Interop;

namespace Voxear;

/// <summary>An Obj's parent and children, as the engine's obj_hierarchy.rs answers for them and sets the parent.</summary>
internal static class ObjHierarchy
{
	private const int ParentValue = 0;
	private const int ChildCountValue = 1;
	private const int ChildValue = 2;
	private const int SetParentText = 3;

	public static Obj ParentOf(Obj obj)
	{
		return NativeComponent.Get(NativeComponent.Hierarchy, obj, ParentValue, out var parent) ? NativeComponent.ObjOf(parent) : Obj.None;
	}

	/// <summary>Puts an Obj inside another, or at the top with None, as Obj.SetParent says.</summary>
	public static void SetParent(Obj obj, Obj parent, bool keepWorldPlace)
	{
		if (!obj.IsValid)
		{
			throw new InvalidOperationException("An Obj that is not valid cannot go inside another.");
		}

		var asked = $"{parent.Id.ToString(CultureInfo.InvariantCulture)} {(keepWorldPlace ? 1 : 0)}";
		var refused = NativeComponent.Ask(NativeComponent.Hierarchy, obj, SetParentText, asked);
		if (!string.IsNullOrEmpty(refused))
		{
			throw new InvalidOperationException(refused);
		}
	}

	public static Obj[] ChildrenOf(Obj obj)
	{
		var count = (int)NativeComponent.Number(NativeComponent.Hierarchy, obj, ChildCountValue, 0f);
		var children = new Obj[count];
		for (var index = 0; index < count; index++)
		{
			NativeComponent.Get(NativeComponent.Hierarchy, obj, index << 8 | ChildValue, out var child);
			children[index] = NativeComponent.ObjOf(child);
		}
		return children;
	}
}
