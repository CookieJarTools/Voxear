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
