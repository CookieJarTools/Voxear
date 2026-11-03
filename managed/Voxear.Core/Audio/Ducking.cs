namespace Voxear;

/// <summary>
/// How the music turns down while the effects are loud (see Audio.Ducking): by <paramref name="Amount"/> decibels,
/// below 0, while the effects are louder than <paramref name="Threshold"/>, from 0 to 1 for full, going down over
/// <paramref name="Attack"/> seconds and coming back over <paramref name="Release"/>.
/// </summary>
public readonly record struct Ducking(float Amount, float Threshold = 0.1f, float Attack = 0.05f, float Release = 0.5f);
