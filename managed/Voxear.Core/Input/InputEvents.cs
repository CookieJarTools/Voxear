using System.Collections.Generic;
using System.Threading.Tasks;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// What Input tells rather than is asked, once a frame before the behaviours' Update: the events of each action
/// someone listens to, the scheme changing, and a rebinding finishing.
/// </summary>
internal static class InputEvents
{
	private static readonly List<InputAction> watched = new List<InputAction>();

	private static ControlScheme? scheme;

	private static TaskCompletionSource<string?>? rebinding;

	public static void Watch(InputAction action)
	{
		if (!watched.Contains(action))
		{
			watched.Add(action);
		}
	}

	/// <summary>Waits for the binding Input is listening for, finishing with the control taken, or null when given up.</summary>
	public static Task<string?> AwaitRebind()
	{
		rebinding?.TrySetResult(null);
		rebinding = new TaskCompletionSource<string?>();
		return rebinding.Task;
	}

	public static void Dispatch()
	{
		watched.RemoveAll(action => !action.HasListeners);
		foreach (var action in watched.ToArray())
		{
			action.Raise();
		}
		var now = Input.Scheme;
		if (scheme != null && scheme != now)
		{
			Input.RaiseSchemeChanged(now);
		}
		scheme = now;
		FinishRebind();
	}

	private static void FinishRebind()
	{
		if (rebinding == null)
		{
			return;
		}
		var done = NativeInput.Change(NativeInput.ChangePoll);
		if (done == 0)
		{
			return;
		}
		var finished = rebinding;
		rebinding = null;
		finished.TrySetResult(done == 1 ? NativeInput.Text(NativeInput.TextRebound) : null);
	}
}
