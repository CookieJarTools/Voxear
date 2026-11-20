namespace Voxear;

/// <summary>How much of the frame a character in a crowd gets, by how near it is to the camera and the players.</summary>
public enum CrowdTier
{
	/// <summary>By its distance, as Project Settings ▸ Crowds sets them. Only a Crowd Member's own choice is this.</summary>
	ByDistance,

	/// <summary>Everything, every step and frame.</summary>
	Near,

	/// <summary>Planned and posed every other step and frame.</summary>
	Middle,

	/// <summary>Planned, moved and posed less often still, its clips played on the GPU.</summary>
	Far,
}
