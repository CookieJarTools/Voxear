namespace Voxear;

/// <summary>How many characters are in each tier of the crowd this frame, and how many are kept as records far off.</summary>
public readonly record struct CrowdCounts(int Near, int Middle, int Far, int Records);
