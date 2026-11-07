using System.Numerics;
using System.Text;

namespace Voxear.Interop;

/// <summary>
/// The engine's networked play, for Net: what each call's number is, as the engine's net/scripts.rs has them.
/// Without the engine, as in tests, there is never a session.
/// </summary>
internal static unsafe class NativeNet
{
	public const int Role = 0;
	public const int Connected = 1;
	public const int LocalPlayer = 2;
	public const int PlayerCount = 3;
	public const int PlayerAt = 4;
	public const int StartServer = 5;
	public const int StartHost = 6;
	public const int Connect = 7;
	public const int Stop = 8;
	public const int TakeEvent = 9;
	public const int Input = 10;
	public const int Tick = 11;
	public const int RoundTrip = 12;
	public const int RpcTick = 13;
	public const int NetId = 14;
	public const int ObjIndex = 15;
	public const int ObjGeneration = 16;
	public const int Replaying = 17;
	public const int Authority = 18;
	public const int Aim = 19;
	public const int SetAim = 20;

	public const int ConnectedEvent = 1;
	public const int DisconnectedEvent = 2;
	public const int JoinedEvent = 3;
	public const int LeftEvent = 4;

	public static int Call(int what, int a = 0, int b = 0)
	{
		return Native.IsConnected ? Native.Api->NetCall(what, a, b, null, 0, null) : 0;
	}

	/// <summary>A call that gives back floats besides its answer.</summary>
	public static int Call(int what, int a, int b, out Vector4 values)
	{
		var read = default(Vector4);
		var answer = Native.IsConnected ? Native.Api->NetCall(what, a, b, null, 0, (float*)&read) : 0;
		values = read;
		return answer;
	}

	public static int Call(int what, string text)
	{
		if (!Native.IsConnected)
		{
			return 0;
		}
		var bytes = Encoding.UTF8.GetBytes(text);
		fixed (byte* written = bytes)
		{
			return Native.Api->NetCall(what, 0, 0, written, bytes.Length, null);
		}
	}
}
