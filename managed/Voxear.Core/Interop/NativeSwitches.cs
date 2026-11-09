namespace Voxear.Interop;

/// <summary>
/// Switching Objs and their components on and off, for Obj.Active and a component's Enabled. Without the engine, as in
/// tests, everything is on.
/// </summary>
internal static unsafe class NativeSwitches
{
	private const int ObjSelf = -1;
	private const int InHierarchy = -2;
	private const int Read = -1;

	public static bool Active(Obj obj)
	{
		return Get(obj, ObjSelf);
	}

	public static bool ActiveInHierarchy(Obj obj)
	{
		return Get(obj, InHierarchy);
	}

	public static void SetActive(Obj obj, bool active)
	{
		Set(obj, ObjSelf, active);
	}

	public static bool Enabled(Obj obj, EngineComponent component)
	{
		return Get(obj, (int)component);
	}

	public static void SetEnabled(Obj obj, EngineComponent component, bool enabled)
	{
		Set(obj, (int)component, enabled);
	}

	private static bool Get(Obj obj, int what)
	{
		return !Native.IsConnected || Native.Api->ObjSwitch(obj.Id, what, Read) != 0;
	}

	private static void Set(Obj obj, int what, bool on)
	{
		if (Native.IsConnected)
		{
			Native.Api->ObjSwitch(obj.Id, what, on ? 1 : 0);
		}
	}
}
