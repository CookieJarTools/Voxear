using System.Numerics;

namespace Voxear;

/// <summary>A dungeon's entrance at the surface: its kind, the middle of its threshold, and the way it faces out.</summary>
public readonly record struct DungeonEntrance(DungeonEntranceKind Kind, Vector3 Position, Vector3 Facing);
