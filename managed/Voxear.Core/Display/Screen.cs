using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The game's window: how big it is, and whether it fills the screen. A build profile sets how the game starts;
/// SetResolution changes it while the game runs, as a settings menu does, at the end of the frame. In the editor,
/// Width and Height are the Game view's, and SetResolution changes nothing.
/// </summary>
public static class Screen
{
	/// <summary>The window's width, in pixels.</summary>
	public static int Width => Native.Screen().Width;

	/// <summary>The window's height, in pixels.</summary>
	public static int Height => Native.Screen().Height;

	/// <summary>Whether the game fills the screen, rather than showing in a window.</summary>
	public static bool Fullscreen
	{
		get { return Native.Screen().Fullscreen; }
		set { SetResolution(Width, Height, value); }
	}

	/// <summary>Asks for a window of this size, in pixels, or the whole screen.</summary>
	public static void SetResolution(int width, int height, bool fullscreen)
	{
		Native.SetScreen(width, height, fullscreen);
	}
}
