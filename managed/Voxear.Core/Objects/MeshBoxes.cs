using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>The box round the mesh an Obj draws, as the engine's mesh_boxes.rs answers for it.</summary>
internal static class MeshBoxes
{
	private const int LeastValue = 3;
	private const int MostValue = 4;

	/// <summary>The box where the Obj is in the world, or null without a mesh, or before the renderer has it.</summary>
	public static OrientedBox? Of(Obj obj)
	{
		if (!NativeComponent.Get(NativeComponent.MeshRenderer, obj, LeastValue, out var least))
		{
			return null;
		}

		NativeComponent.Get(NativeComponent.MeshRenderer, obj, MostValue, out var most);
		var placed = Native.GetWorldTransform(obj.Id);
		var frame = Matrix4x4.CreateScale(placed.Scale)
			* Matrix4x4.CreateFromQuaternion(placed.Rotation)
			* Matrix4x4.CreateTranslation(placed.Position);
		return new OrientedBox(frame, least.AsVector3(), most.AsVector3());
	}
}
