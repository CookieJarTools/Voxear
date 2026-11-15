using System.Numerics;

namespace Voxear;

/// <summary>A light of a dungeon: what it is, and where it stands.</summary>
public readonly record struct DungeonLight(DungeonLightKind Kind, Vector3 Position);
