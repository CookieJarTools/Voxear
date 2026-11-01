using System;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;
using Voxear.Interop;

namespace Voxear.Scripting
{
	/// <summary>
	/// The functions the engine calls. An exception must never cross back into native code, so each
	/// entry point catches everything and reports it through the log.
	/// </summary>
	internal static unsafe class EntryPoints
	{
		/// <summary>InspectorCall's last answer, kept for a caller whose buffer was too small for it.</summary>
		private static byte[] lastAnswer = Array.Empty<byte>();

		[UnmanagedCallersOnly(CallConvs = new[] { typeof(CallConvCdecl) })]
		public static int LoadGameAssembly(byte* path, int length)
		{
			try
			{
				GameScripts.Load(Native.Utf8(path, length));
				return 0;
			}
			catch (Exception exception)
			{
				Log.Exception(exception);
				return 1;
			}
		}

		/// <summary>
		/// Loads a new build of the game scripts in place of the running one (see GameScripts.Reload): 0 when it did, 1
		/// when the new build could not be loaded and nothing changed, and 2 when the reload failed partway, which is
		/// logged.
		/// </summary>
		[UnmanagedCallersOnly(CallConvs = new[] { typeof(CallConvCdecl) })]
		public static int ReloadGameAssembly(byte* path, int length, uint* handles, uint* enabled, BehaviourInfo* infos, int count)
		{
			try
			{
				var reloaded = GameScripts.Reload(
					Native.Utf8(path, length),
					new ReadOnlySpan<uint>(handles, count),
					new ReadOnlySpan<uint>(enabled, count),
					new Span<BehaviourInfo>(infos, count));
				return reloaded ? 0 : 1;
			}
			catch (Exception exception)
			{
				Log.Exception(exception);
				return 2;
			}
		}

		[UnmanagedCallersOnly(CallConvs = new[] { typeof(CallConvCdecl) })]
		public static int CreateBehaviour(byte* typeName, int length, ulong obj, byte* fields, int fieldsLength, BehaviourInfo* info)
		{
			try
			{
				var json = fields == null ? null : Native.Utf8(fields, fieldsLength);
				return BehaviourTable.Create(Native.Utf8(typeName, length), new Obj(obj), json, info);
			}
			catch (Exception exception)
			{
				Log.Exception(exception);
				return -1;
			}
		}

		[UnmanagedCallersOnly(CallConvs = new[] { typeof(CallConvCdecl) })]
		public static void Update()
		{
			try
			{
				Loading.Update();
				UiElements.Dispatch();
				MainThreadContext.Instance.Run();
				GameScripts.Settle();
			}
			catch (Exception exception)
			{
				Log.Exception(exception);
			}
		}

		/// <summary>
		/// Writes the game scripts' content roots as UTF-8 into the buffer when they fit, and returns their length
		/// in bytes either way, so the engine can call again with a buffer large enough.
		/// </summary>
		/// <summary>A level change the editor asks for, made as a script would make it so every Level event fires.</summary>
		[UnmanagedCallersOnly(CallConvs = new[] { typeof(CallConvCdecl) })]
		public static void SwitchLevel(byte* level, int length)
		{
			try
			{
				Level.LoadAsync(Native.Utf8(level, length));
			}
			catch (Exception exception)
			{
				Log.Exception(exception);
			}
		}

		/// <summary>
		/// Runs a [Command] method the editor's palette picked: 0 when it ran, 1 when it could not be found, and 2 when
		/// it threw, which is logged.
		/// </summary>
		[UnmanagedCallersOnly(CallConvs = new[] { typeof(CallConvCdecl) })]
		public static int RunCommand(byte* type, int typeLength, byte* method, int methodLength)
		{
			try
			{
				return Commands.Run(Native.Utf8(type, typeLength), Native.Utf8(method, methodLength)) ? 0 : 1;
			}
			catch (Exception exception)
			{
				Log.Exception(exception);
				return 2;
			}
		}

		/// <summary>
		/// Runs a check a guide's step names: 0 when it passes; 1 when it does not, with what is still wrong written into
		/// buffer, cut to fit, and its length into written; 2 when there is no such check; 3 when it threw, which is logged.
		/// </summary>
		[UnmanagedCallersOnly(CallConvs = new[] { typeof(CallConvCdecl) })]
		public static int RunCheck(byte* type, int typeLength, byte* method, int methodLength, byte* buffer, int capacity, int* written)
		{
			*written = 0;
			try
			{
				if (!GuideChecks.Run(Native.Utf8(type, typeLength), Native.Utf8(method, methodLength), out var passed, out var failure))
				{
					return 2;
				}
				if (passed)
				{
					return 0;
				}

				*written = GuideChecks.Write(failure, new Span<byte>(buffer, capacity));
				return 1;
			}
			catch (Exception exception)
			{
				Log.Exception(exception);
				return 3;
			}
		}

		/// <summary>
		/// Answers the Inspector's request about a behaviour in edit mode (see InspectorCalls), writing the answer into
		/// buffer when it fits and returning its length either way. A request of no bytes asks for the last answer again,
		/// for a caller whose buffer was too small, so nothing runs twice.
		/// </summary>
		[UnmanagedCallersOnly(CallConvs = new[] { typeof(CallConvCdecl) })]
		public static int InspectorCall(byte* request, int length, byte* buffer, int capacity)
		{
			try
			{
				if (length > 0)
				{
					lastAnswer = Encoding.UTF8.GetBytes(InspectorCalls.Answer(Native.Utf8(request, length)));
				}
				if (lastAnswer.Length <= capacity)
				{
					lastAnswer.CopyTo(new Span<byte>(buffer, capacity));
				}
				return lastAnswer.Length;
			}
			catch (Exception exception)
			{
				Log.Exception(exception);
				return -1;
			}
		}

		/// <summary>
		/// The managed heap, for the memory profiler: the bytes in use now, those committed, all ever allocated, and each
		/// generation's size after the last collection (0, 1 and 2, the large objects' and the pinned ones'). Writes as
		/// many as count allows and returns how many it wrote.
		/// </summary>
		[UnmanagedCallersOnly(CallConvs = new[] { typeof(CallConvCdecl) })]
		public static int ManagedMemory(ulong* values, int count)
		{
			var info = GC.GetGCMemoryInfo();
			var measured = new long[3 + info.GenerationInfo.Length];
			(measured[0], measured[1], measured[2]) = (GC.GetTotalMemory(false), info.TotalCommittedBytes, GC.GetTotalAllocatedBytes());
			for (var generation = 0; generation < info.GenerationInfo.Length; generation++)
			{
				measured[3 + generation] = info.GenerationInfo[generation].SizeAfterBytes;
			}
			var written = Math.Min(count, measured.Length);
			for (var index = 0; index < written; index++)
			{
				values[index] = (ulong)Math.Max(measured[index], 0);
			}
			return written;
		}

		[UnmanagedCallersOnly(CallConvs = new[] { typeof(CallConvCdecl) })]
		public static int ContentRoots(byte* buffer, int capacity)
		{
			try
			{
				var bytes = Encoding.UTF8.GetBytes(Voxear.ContentRoots.Describe(GameScripts.Assemblies));
				if (bytes.Length <= capacity)
				{
					bytes.CopyTo(new Span<byte>(buffer, capacity));
				}

				return bytes.Length;
			}
			catch (Exception exception)
			{
				Log.Exception(exception);
				return -1;
			}
		}

		/// <summary>Gives an entity a component a scene or prefab names, with its field values: 0 when it did, -1 when not.</summary>
		[UnmanagedCallersOnly(CallConvs = new[] { typeof(CallConvCdecl) })]
		public static int AddComponent(byte* typeName, int length, ulong entity, byte* fields, int fieldsLength)
		{
			try
			{
				var json = fields == null ? null : Native.Utf8(fields, fieldsLength);
				return SceneComponentTypes.Add(Native.Utf8(typeName, length), new Entity(entity), json) ? 0 : -1;
			}
			catch (Exception exception)
			{
				Log.Exception(exception);
				return -1;
			}
		}

		/// <summary>
		/// Runs a line typed in a development build's console, printing what it did as log lines: 0 when it ran, 1 when
		/// nothing has its first word as a name, and 2 when it failed, which is logged.
		/// </summary>
		[UnmanagedCallersOnly(CallConvs = new[] { typeof(CallConvCdecl) })]
		public static int ConsoleRun(byte* line, int length)
		{
			try
			{
				var printed = DevConsole.Run(Native.Utf8(line, length));
				if (printed == null)
				{
					return 1;
				}
				foreach (var (level, text) in printed)
				{
					Native.Log(level, text, string.Empty, default);
				}
				return 0;
			}
			catch (Exception exception)
			{
				Log.Exception(exception);
				return 2;
			}
		}

		/// <summary>The console's commands and variables, as DevConsole.Names writes them, when they fit; their length either way.</summary>
		[UnmanagedCallersOnly(CallConvs = new[] { typeof(CallConvCdecl) })]
		public static int ConsoleNames(byte* buffer, int capacity)
		{
			try
			{
				var bytes = Encoding.UTF8.GetBytes(DevConsole.Names(GameScripts.Assemblies));
				if (bytes.Length <= capacity)
				{
					bytes.CopyTo(new Span<byte>(buffer, capacity));
				}
				return bytes.Length;
			}
			catch (Exception exception)
			{
				Log.Exception(exception);
				return -1;
			}
		}

		/// <summary>Runs the game's systems of one stage, each guarded on its own, so one that throws stops no other.</summary>
		[UnmanagedCallersOnly(CallConvs = new[] { typeof(CallConvCdecl) })]
		public static void RunSystems(int stage)
		{
			try
			{
				Systems.Run((Stage)stage);
			}
			catch (Exception exception)
			{
				Log.Exception(exception);
			}
		}

		[UnmanagedCallersOnly(CallConvs = new[] { typeof(CallConvCdecl) })]
		public static void Invoke(int phase, uint* handles, int count)
		{
			BehaviourTable.Invoke((Phase)phase, new ReadOnlySpan<uint>(handles, count));
		}

		[UnmanagedCallersOnly(CallConvs = new[] { typeof(CallConvCdecl) })]
		public static void Release(uint* handles, int count)
		{
			BehaviourTable.Release(new ReadOnlySpan<uint>(handles, count));
		}
	}
}
