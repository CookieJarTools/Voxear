using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// What players may ask of the server's endless Voxel Volumes. On a player's machine every <c>Voxels</c> change is a
/// request, which the server checks before it makes it: the easy way by these rules, the full way by
/// <see cref="Check"/>, which sees each request whole and accepts or refuses it. A request refused is let go on the
/// player's machine. Set them on the server.
/// <code>
/// // The easy way: players dig, build and cut within 8 m of themselves, nothing larger than a metre and a half.
/// VoxelRequests.Allowed = VoxelRequestKinds.Building | VoxelRequestKinds.Cut;
/// VoxelRequests.Reach = 8f;
/// VoxelRequests.Largest = 1.5f;
///
/// // The full way: only a player carrying a charge may blast, and nobody digs in the town.
/// VoxelRequests.Check = request => request.Kind switch
/// {
///     VoxelRequestKind.Blast when !Charges.Has(request.Player) => VoxelAnswer.Refuse("no charge"),
///     VoxelRequestKind.Dig or VoxelRequestKind.Carve when Town.Contains(request.At) => VoxelAnswer.Refuse("in the town"),
///     _ => VoxelAnswer.Accept,
/// };
/// </code>
/// </summary>
public static class VoxelRequests
{
	private static Func<VoxelRequest, VoxelAnswer>? check;

	/// <summary>The kinds of request players may make; the rest are refused. Every kind unless set.</summary>
	public static VoxelRequestKinds Allowed
	{
		get { return (VoxelRequestKinds)NativeNet.Call(NativeNet.RequestKinds); }
		set { NativeNet.Call(NativeNet.RequestKinds, (int)value, 1); }
	}

	/// <summary>How far from the asking player's own Obj a request may be, in metres; 0 for anywhere.</summary>
	public static float Reach
	{
		get { return Length(NativeNet.RequestReach); }
		set { NativeNet.Call(NativeNet.RequestReach, BitConverter.SingleToInt32Bits(value), 1); }
	}

	/// <summary>The largest ball's radius or box's side a request may have, in metres: 24 unless set.</summary>
	public static float Largest
	{
		get { return Length(NativeNet.RequestLargest); }
		set { NativeNet.Call(NativeNet.RequestLargest, BitConverter.SingleToInt32Bits(value), 1); }
	}

	/// <summary>The most points one request may set: 4096 unless set.</summary>
	public static int MostPoints
	{
		get { return NativeNet.Call(NativeNet.RequestPoints); }
		set { NativeNet.Call(NativeNet.RequestPoints, value, 1); }
	}

	/// <summary>
	/// The game's own check of every request the rules let through, on the server: the request is held until it
	/// answers, within the frame it came in. None to make every request the rules let through.
	/// </summary>
	public static Func<VoxelRequest, VoxelAnswer>? Check
	{
		get { return check; }
		set
		{
			check = value;
			NativeNet.Call(NativeNet.ScriptsCheck, value is null ? 0 : 1);
		}
	}

	private static float Length(int what)
	{
		NativeNet.Call(what, 0, 0, out var values);
		return values.X;
	}

	/// <summary>Answers each request held for <see cref="Check"/>, once a frame.</summary>
	internal static void Dispatch()
	{
		if (check is null)
		{
			return;
		}
		while (true)
		{
			var id = NativeNet.Call(NativeNet.TakeRequest, 0, 0, out var place);
			if (id == 0)
			{
				return;
			}
			var kind = NativeNet.Call(NativeNet.RequestInfo, id, 0, out var info);
			if (kind < 0)
			{
				continue;
			}
			VoxelMaterial? material = info.Z < 0f ? null : (VoxelMaterial)(int)info.Z;
			var request = new VoxelRequest(new NetPlayer((int)info.X), (VoxelRequestKind)kind, new Vector3(place.X, place.Y, place.Z), place.W, (int)info.Y, material);
			var answer = Answer(request);
			NativeNet.Call(NativeNet.AnswerRequest, id, answer.Accepted ? 1 : 0, answer.Reason);
		}
	}

	private static VoxelAnswer Answer(VoxelRequest request)
	{
		try
		{
			return check?.Invoke(request) ?? VoxelAnswer.Accept;
		}
		catch (Exception exception)
		{
			Log.Exception(exception);
			return VoxelAnswer.Refuse("the check failed");
		}
	}
}
