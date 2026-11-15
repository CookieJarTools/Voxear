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

	/// <summary>
	/// Whether the game asks for HDR output, as an options menu sets it. Off by default. Where the display or its driver
	/// offers none of what is asked, the game shows SDR, and HdrOutput says so.
	/// </summary>
	public static HdrMode Hdr
	{
		get { return (HdrMode)Native.Hdr(null).Mode; }
		set { SetHdr(value, PaperWhite, PeakBrightness); }
	}

	/// <summary>What the window shows now: SDR, or the HDR it was given.</summary>
	public static HdrOutput HdrOutput => (Voxear.HdrOutput)Native.Hdr(null).Output;

	/// <summary>Whether the display offers any HDR output, so an options menu can show its HDR settings.</summary>
	public static bool HdrAvailable => Native.Hdr(null).Offered != 0;

	/// <summary>
	/// How bright white paper is on an HDR display, in nits: the brightness the picture's midtones and the UI are shown at.
	/// 200 by default; 80 to 500 suit most rooms.
	/// </summary>
	public static float PaperWhite
	{
		get { return Native.Hdr(null).PaperWhite; }
		set { SetHdr(Hdr, value, PeakBrightness); }
	}

	/// <summary>The brightest the display shows, in nits, which highlights roll off towards. 1000 by default.</summary>
	public static float PeakBrightness
	{
		get { return Native.Hdr(null).Peak; }
		set { SetHdr(Hdr, PaperWhite, value); }
	}

	/// <summary>Sets HDR output, paper white and the peak together, from the next frame.</summary>
	public static void SetHdr(HdrMode mode, float paperWhite, float peakBrightness)
	{
		var asked = new HdrInfoData { Mode = (int)mode, PaperWhite = paperWhite, Peak = peakBrightness };
		Native.Hdr(asked);
	}
}
