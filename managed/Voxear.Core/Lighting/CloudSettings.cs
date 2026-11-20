using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>The sky's clouds: a layer of them between two heights, drifting with the wind. None with no cover.</summary>
public sealed class CloudSettings
{
	/// <summary>How much of the sky they cover, from 0 for a clear sky to 1 for overcast.</summary>
	public float Cover = 0f;

	/// <summary>How high their undersides are over the level's ground, in metres.</summary>
	public float Height = 1200f;

	/// <summary>How tall they are from their undersides to their tops, in metres.</summary>
	public float Thickness = 700f;

	/// <summary>How far across a cloud is, in metres.</summary>
	public float Size = 900f;

	/// <summary>How thick they are to look through: 1 for fair-weather clouds, less for thin ones.</summary>
	public float Density = 1f;

	/// <summary>How fast they drift, in metres a second, the way the level's wind blows.</summary>
	public float Drift = 8f;

	/// <summary>How dark their shadows on the ground are, from 0 for none to 1.</summary>
	public float Shadows = 0.8f;

	/// <summary>Soft clouds, or clouds built of cubes.</summary>
	public CloudKind Kind = CloudKind.Soft;

	/// <summary>How wide one cube of a cloud built of them is, in metres.</summary>
	public float Block = 100f;

	/// <summary>How soft a cloud built of cubes is, from 0 for solid cubes to 1 for cubes of mist.</summary>
	public float Softness = 0f;

	internal void Read(JsonObject from)
	{
		PostJson.Read(from, "cover", ref Cover);
		PostJson.Read(from, "height", ref Height);
		PostJson.Read(from, "thickness", ref Thickness);
		PostJson.Read(from, "size", ref Size);
		PostJson.Read(from, "density", ref Density);
		PostJson.Read(from, "drift", ref Drift);
		PostJson.Read(from, "shadows", ref Shadows);
		PostJson.ReadChoice(from, "kind", ref Kind);
		PostJson.Read(from, "block", ref Block);
		PostJson.Read(from, "softness", ref Softness);
	}

	internal void Write(JsonObject to)
	{
		to["cover"] = Cover;
		to["height"] = Height;
		to["thickness"] = Thickness;
		to["size"] = Size;
		to["density"] = Density;
		to["drift"] = Drift;
		to["shadows"] = Shadows;
		to["kind"] = PostJson.Of(Kind);
		to["block"] = Block;
		to["softness"] = Softness;
	}
}
