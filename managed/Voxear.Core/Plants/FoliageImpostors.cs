namespace Voxear;

/// <summary>When a Foliage's far plants are drawn as impostors: flat pictures of them that turn to face the camera.</summary>
public enum FoliageImpostors
{
	/// <summary>As the engine chooses: for plants as large as trees, past a third of the draw distance, never for grass.</summary>
	Auto,

	/// <summary>Never: always the plant's own mesh.</summary>
	Off,

	/// <summary>From the Foliage's ImpostorDistance.</summary>
	From,
}
