namespace Voxear;

/// <summary>What fills a Camera's picture where it sees nothing.</summary>
public enum CameraBackground
{
	/// <summary>The sky, as the level's lighting draws it.</summary>
	Sky,

	/// <summary>One colour: the Camera's BackgroundColor.</summary>
	Color,

	/// <summary>Nothing: what was drawn there before shows, as under a Camera drawn over another.</summary>
	Nothing,
}
