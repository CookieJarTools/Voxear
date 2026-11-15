namespace Voxear;

/// <summary>What of a Light a lighting bake keeps.</summary>
public enum LightMode
{
	/// <summary>Nothing: it lights in real time, and only the probes round the camera take its bounce.</summary>
	Realtime,

	/// <summary>Its bounce: it lights directly in real time, and the bake keeps the light it bounces. The default.</summary>
	Mixed,

	/// <summary>Its bounce, and its direct light on lightmapped surfaces; moving Objs are still lit by it in real time.</summary>
	Baked,
}
