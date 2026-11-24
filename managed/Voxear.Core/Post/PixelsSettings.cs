using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>
/// Drawing in pixels: every pass draws the scene at a few pixels to the screen's many, and the picture is enlarged with
/// each drawn pixel a square block of screen pixels, never blurred. TAA, FXAA and sharpening are off while it is on.
/// <code>
/// Post.Change(post =&gt; post.Pixels.DrawAt(4));
/// </code>
/// </summary>
public sealed class PixelsSettings
{
	/// <summary>Whether the scene is drawn in pixels.</summary>
	public bool Enabled;

	/// <summary>Screen pixels each drawn pixel covers, each way, 1 to 32.</summary>
	public int Size = 4;

	/// <summary>About how many rows of pixels to draw, in place of Size: the whole pixel size nearest to giving them. 0 leaves Size to say.</summary>
	public int Height;

	/// <summary>Keeps an orthographic camera on the grid of its pixels, so they do not crawl or shimmer as it glides.</summary>
	public bool Snap;

	/// <summary>Turns drawing in pixels on, each drawn pixel <paramref name="size"/> screen pixels across.</summary>
	public void DrawAt(int size)
	{
		Enabled = true;
		Size = size;
		Height = 0;
	}

	internal void Read(JsonObject from)
	{
		PostJson.Read(from, "enabled", ref Enabled);
		PostJson.Read(from, "size", ref Size);
		PostJson.Read(from, "height", ref Height);
		PostJson.Read(from, "snap", ref Snap);
	}

	internal void Write(JsonObject to)
	{
		to["enabled"] = Enabled;
		to["size"] = Size;
		to["height"] = Height;
		to["snap"] = Snap;
	}
}
