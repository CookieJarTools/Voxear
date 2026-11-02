using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The mouse pointer over the game: whether it shows, and whether it is held in the window. A build and vox run
/// make what is set at the end of the frame. The editor leaves the mouse free, so it can still be used to work.
/// </summary>
public static class Cursor
{
	/// <summary>Whether the pointer shows. A locked pointer never shows.</summary>
	public static bool Visible
	{
		get { return Native.Cursor().Visible; }
		set { Native.SetCursor(value, Lock); }
	}

	/// <summary>Whether the pointer is free, held in place, or kept inside the window.</summary>
	public static CursorLock Lock
	{
		get { return Native.Cursor().Lock; }
		set { Native.SetCursor(Visible, value); }
	}
}
