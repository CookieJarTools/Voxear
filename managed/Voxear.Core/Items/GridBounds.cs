namespace Voxear;

/// <summary>The part of a grid of items that has items in it: its top left place, and how wide and high it is.</summary>
internal readonly record struct GridBounds(int Left, int Top, int Wide, int High);
