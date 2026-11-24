namespace Voxear;

/// <summary>What a shader pack's option is, and so how a menu sets it.</summary>
public enum ShaderPackOptionKind
{
	/// <summary>A number between Low and High: a slider.</summary>
	Number,

	/// <summary>On or off: a toggle.</summary>
	Toggle,

	/// <summary>One of its Choices: a dropdown.</summary>
	Choice,

	/// <summary>A colour: a swatch.</summary>
	Color,
}
