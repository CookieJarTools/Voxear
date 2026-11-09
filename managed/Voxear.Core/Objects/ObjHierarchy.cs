using Voxear.Interop;

namespace Voxear;

/// <summary>An Obj's parent and children, as the engine's obj_hierarchy.rs answers for them.</summary>
internal static class ObjHierarchy
{
	private const int ParentValue = 0;
	private const int ChildCountValue = 1;
	private const int ChildValue = 2;

	public static Obj ParentOf(Obj obj)
	{
		return NativeComponent.Get(NativeComponent.Hierarchy, obj, ParentValue, out var parent) ? NativeComponent.ObjOf(parent) : Obj.None;
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
