namespace Voxear;

/// <summary>How the shader pack worn stands.</summary>
public enum ShaderPackStanding
{
	/// <summary>None is worn.</summary>
	None,

	/// <summary>Its programs are compiling; the pack worn before, if any, is drawn until they have.</summary>
	Compiling,

	/// <summary>It is drawn.</summary>
	Drawn,

	/// <summary>Its programs did not compile; ShaderPacks.Error says why, and the pack worn before is drawn.</summary>
	Failed,
}
