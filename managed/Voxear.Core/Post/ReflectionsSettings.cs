using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>Reflections of what is round smooth surfaces: traced where the GPU can, otherwise what is on screen. They need TAA.</summary>
public sealed class ReflectionsSettings
{
	/// <summary>Whether they are on.</summary>
	public bool Enabled = true;

	/// <summary>How far rays go, in metres.</summary>
	public float MaxDistance = 30f;

	/// <summary>How thick things on screen count as, in metres.</summary>
	public float Thickness = 0.3f;

	/// <summary>Surfaces rougher than this, from 0 to 1, show no screen-space reflections.</summary>
	public float MaxRoughness = 0.6f;

	/// <summary>The steps each ray takes.</summary>
	public int Steps = 32;

	internal void Read(JsonObject from)
	{
		PostJson.Read(from, "enabled", ref Enabled);
		PostJson.Read(from, "max_distance", ref MaxDistance);
		PostJson.Read(from, "thickness", ref Thickness);
		PostJson.Read(from, "max_roughness", ref MaxRoughness);
		PostJson.Read(from, "steps", ref Steps);
	}

	internal void Write(JsonObject to)
	{
		to["enabled"] = Enabled;
		to["max_distance"] = MaxDistance;
		to["thickness"] = Thickness;
		to["max_roughness"] = MaxRoughness;
		to["steps"] = Steps;
	}
}
