using System.Collections.Generic;
using System.Numerics;

namespace Voxear;

/// <summary>
/// A tree that came down and broke where it landed: the Block World it fell from, where it was cut, the way it fell,
/// where its middle landed, and the items it broke into, lying on the ground. Take some away, or drop more, to change
/// what it gave.
/// </summary>
public sealed record TreeFelled(Obj World, Vector3 Cut, Vector3 Away, Vector3 Landed, IReadOnlyList<ItemDrop> Drops);
