using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>Contact shadows: the small shadows the sun casts where things touch, which its shadow map is too coarse to hold.</summary>
public sealed class ContactShadowsSettings
{
	/// <summary>Whether they are on.</summary>
	public bool Enabled = true;

	/// <summary>How far each pixel looks towards the sun, in metres.</summary>
	public float Length = 0.3f;

	/// <summary>How thick something on screen counts as, in metres.</summary>
	public float Thickness = 0.15f;

	/// <summary>How many steps the march towards the sun takes.</summary>
	public int Steps = 12;

	internal void Read(JsonObject from)
	{
		PostJson.Read(from, "enabled", ref Enabled);
		PostJson.Read(from, "length", ref Length);
		PostJson.Read(from, "thickness", ref Thickness);
		PostJson.Read(from, "steps", ref Steps);
	}

	internal void Write(JsonObject to)
	{
		to["enabled"] = Enabled;
		to["length"] = Length;
		to["thickness"] = Thickness;
		to["steps"] = Steps;
	}
}
