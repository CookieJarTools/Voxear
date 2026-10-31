using System;

namespace Voxear
{
	/// <summary>A level change that failed, thrown where it is awaited. LevelLoad.Error says the same.</summary>
	public sealed class LevelLoadException : Exception
	{
		public LevelLoadException(string level, string error) : base($"Could not load {level}: {error}")
		{
			Level = level;
		}

		public string Level { get; }
	}
}
