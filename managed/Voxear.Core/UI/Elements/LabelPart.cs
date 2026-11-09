namespace Voxear;

/// <summary>A stretch of a LocalizedLabel's text, or the place of the action among its values whose controls show there.</summary>
internal readonly record struct LabelPart(string Text, int Action);
