using System;
using System.Text;

namespace Voxear.Interop
{
	/// <summary>Thin managed wrappers over the engine's function table.</summary>
	internal static unsafe class Native
	{
		public static NativeApi* Api;

		public static bool IsConnected => Api != null;

		public static void Log(LogLevel level, string message)
		{
			if (!IsConnected)
			{
				Console.WriteLine($"[{level}] {message}");
				return;
			}

			var bytes = Encoding.UTF8.GetBytes(message);
			fixed (byte* text = bytes)
			{
				Api->Log((int)level, text, bytes.Length);
			}
		}

		public static string ObjName(ulong obj)
		{
			var length = Api->ObjName(obj, null, 0);
			if (length <= 0)
			{
				return string.Empty;
			}

			var bytes = new byte[length];
			fixed (byte* buffer = bytes)
			{
				Api->ObjName(obj, buffer, length);
			}
			return Encoding.UTF8.GetString(bytes);
		}

		public static Transform GetTransform(ulong obj)
		{
			var transform = Transform.Identity;
			Api->TransformGet(obj, &transform);
			return transform;
		}

		public static void SetTransform(ulong obj, Transform transform)
		{
			Api->TransformSet(obj, &transform);
		}

		public static void SetEnabled(uint behaviour, bool enabled)
		{
			Api->BehaviourSetEnabled(behaviour, enabled ? 1 : 0);
		}

		public static void Destroy(uint behaviour)
		{
			Api->BehaviourDestroy(behaviour);
		}

		public static string Utf8(byte* text, int length)
		{
			return length <= 0 ? string.Empty : Encoding.UTF8.GetString(text, length);
		}
	}
}
