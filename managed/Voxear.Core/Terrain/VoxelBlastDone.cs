namespace Voxear;

/// <summary>
/// What a blast did (<see cref="Voxels.Blast"/>): how many endless Voxel Volumes it reached and how many built voxels
/// it dug out. What comes loose after it falls a moment later, as the volume works it out.
/// </summary>
public readonly record struct VoxelBlastDone(int Volumes, int Dug);
