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
