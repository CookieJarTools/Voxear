using System;

namespace Voxear;

/// <summary>
/// Marks a behaviour's field or property that a save of the scene keeps, such as whether a door is open, so it
/// is as the player left it after loading (see SceneState and Save.WriteScene). On a behaviour class, it keeps
/// every field and property a save keeps of it. On a component struct (IComponent), it keeps that component of
/// every Obj that has one, whole. A field set in the scene file and never changed in play has no need of it.
/// </summary>
[AttributeUsage(AttributeTargets.Field | AttributeTargets.Property | AttributeTargets.Class | AttributeTargets.Struct)]
public sealed class SavedAttribute : Attribute
{
}
