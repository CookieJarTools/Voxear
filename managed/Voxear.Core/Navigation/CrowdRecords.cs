using System;

namespace Voxear;

/// <summary>
/// The level's crowd records: Crowd Members kept as a few numbers while they are far off (Project Settings ▸ Crowds
/// ▸ Records beyond), each made again from its prefab when it comes near. A save of the scene keeps them by itself;
/// these are for a game that keeps its own.
/// </summary>
public static class CrowdRecords
{
	/// <summary>The records as bytes, small: each prefab once, then where each is, where it was going and how fast.</summary>
	public static unsafe byte[] Capture()
	{
		if (!Interop.Native.IsConnected)
		{
			return [];
		}

		var length = Interop.Native.Api->CrowdRecords(0, null, 0, null, 0);
		while (true)
		{
			var buffer = new byte[Math.Max(length, 0)];
			fixed (byte* into = buffer)
			{
				length = Interop.Native.Api->CrowdRecords(0, null, 0, into, buffer.Length);
			}
			if (length <= buffer.Length)
			{
				return length == buffer.Length ? buffer : buffer[..Math.Max(length, 0)];
			}
		}
	}

	/// <summary>Puts back the records <paramref name="records"/> holds, in place of the level's. False where they are not records' bytes.</summary>
	public static unsafe bool Apply(byte[] records)
	{
		if (!Interop.Native.IsConnected || records.Length == 0)
		{
			return false;
		}

		fixed (byte* from = records)
		{
			return Interop.Native.Api->CrowdRecords(1, from, records.Length, null, 0) >= 0;
		}
	}
}
