using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>
/// What the game asked of the GPU as it started, as Project Settings ▸ Graphics has them: fixed once it has started.
/// A game's options change HDR output while it runs with Screen.Hdr.
/// </summary>
public sealed class GraphicsSettings
{
	/// <summary>Whether frames wait for the display.</summary>
	public bool Vsync { get; private set; } = true;

	/// <summary>The HDR output the game starts with.</summary>
	public HdrMode Hdr { get; private set; } = HdrMode.Off;

	/// <summary>Whether the GPU's mesh shaders are used, where it has them.</summary>
	public bool MeshShaders { get; private set; } = true;

	/// <summary>Whether the GPU unpacks compressed content, where it can.</summary>
	public bool GpuDecompression { get; private set; } = true;

	/// <summary>Whether the GPU's ray tracing is used, where it has it.</summary>
	public bool RayTracing { get; private set; } = true;

	internal static GraphicsSettings Read(JsonObject from)
	{
		var settings = new GraphicsSettings();
		settings.Vsync = SettingsJson.Read(from, "vsync", settings.Vsync);
		settings.Hdr = from["hdr"]?.GetValue<string>() switch
		{
			"on" => HdrMode.On,
			"hdr10" => HdrMode.Hdr10,
			"scrgb" => HdrMode.ScRgb,
			_ => HdrMode.Off,
		};
		settings.MeshShaders = SettingsJson.Read(from, "mesh_shaders", settings.MeshShaders);
		settings.GpuDecompression = SettingsJson.Read(from, "gpu_decompression", settings.GpuDecompression);
		settings.RayTracing = SettingsJson.Read(from, "ray_tracing", settings.RayTracing);
		return settings;
	}
}
