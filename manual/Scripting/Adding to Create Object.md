---
order: 20
---

Your scripts, or a package's, can add a prefab of their own to the Hierarchy's **+** menu and the command bar's **New …** choices, beside **Cube** and **Point Light**. Mark a static `Ref<Prefab>` with `[CreateObject]`, giving the menu's folders and a name:

```csharp
using Voxear;

namespace LanternKit;

internal static class LanternObjects
{
	[CreateObject("Lights/Lantern")]
	private static readonly Ref<Prefab> Lantern = new("Prefabs/Lantern.prefab");
}
```

**Lantern** then shows under **Lights** in the **+** menu, and as **New Lantern** in the command bar.

Picked, it makes the prefab's Objs where any new Obj goes, selected and ready to move. They are Objs of the scene, as if you had placed the prefab and unpacked it, not an instance of it: change them freely, and the prefab stays as it was. A prefab nested inside it stays that prefab's instance. One **Ctrl+Z** takes them all away.

In a package's script, the prefab's path is from the package's own folder, so `Prefabs/Lantern.prefab` is the file at `lantern-kit/Prefabs/Lantern.prefab`. Keep such prefabs with the package's assets, not in its `Samples/`, which a project copies. In the game's scripts, the path is from the project's folder.

The menu's path goes in the attribute, and the prefab in the field, because an attribute holds only constants: a prefab given there would have to be a bare string, where a `Ref<Prefab>` says what it names.
