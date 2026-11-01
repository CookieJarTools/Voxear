using System;
using System.Text;
using Voxear.Interop;

namespace Voxear
{
	/// <summary>
	/// One element of a UiDocument, found by name: its text, classes, whether it shows, a slider's or toggle's value,
	/// and clicks. Classes are how it changes look, as in CSS: <c>health.AddClass("low")</c> with a sheet rule for
	/// <c>.low</c>.
	/// </summary>
	public sealed unsafe class UiElement
	{
		private const int Remove = 0;
		private const int Add = 1;
		private const int Toggle = 2;
		private const int Has = 3;

		private Action? clicked;

		internal UiElement(int id)
		{
			Id = id;
		}

		/// <summary>Runs once each time it is clicked, at the start of the next frame.</summary>
		public event Action? Clicked
		{
			add
			{
				clicked += value;
				UiElements.Listen(this);
			}
			remove
			{
				clicked -= value;
			}
		}

		public string Text
		{
			get { return ReadText(); }
			set { WriteText(value); }
		}

		public bool Visible
		{
			get { return Native.IsConnected && Native.Api->UiVisible(Id, -1) != 0; }
			set { Show(value); }
		}

		/// <summary>A slider's value, or a toggle's, 1 when ticked and 0 when not.</summary>
		public float Value
		{
			get { return Native.IsConnected ? Native.Api->UiValue(Id, 0, 0f) : 0f; }
			set { _ = Native.IsConnected ? Native.Api->UiValue(Id, 1, value) : 0f; }
		}

		/// <summary>Whether a toggle is ticked.</summary>
		public bool Checked
		{
			get { return Value != 0f; }
			set { Value = value ? 1f : 0f; }
		}

		/// <summary>Whether it was clicked since the last frame, for code that looks each frame rather than listens.</summary>
		public bool WasClicked => Native.IsConnected && Native.Api->UiClicked(Id) != 0;

		internal int Id { get; }

		internal bool HasListeners => clicked != null;

		public void AddClass(string name)
		{
			ChangeClass(name, Add);
		}

		public void RemoveClass(string name)
		{
			ChangeClass(name, Remove);
		}

		public void ToggleClass(string name)
		{
			ChangeClass(name, Toggle);
		}

		public bool HasClass(string name)
		{
			return ChangeClass(name, Has);
		}

		/// <summary>Declarations that win over its sheets, as a style attribute's: <c>bar.SetStyle("width: 40%")</c>.</summary>
		public void SetStyle(string declarations)
		{
			if (!Native.IsConnected)
			{
				return;
			}

			var bytes = Encoding.UTF8.GetBytes(declarations);
			fixed (byte* text = bytes)
			{
				Native.Api->UiStyle(Id, text, bytes.Length);
			}
		}

		internal void RaiseClicked()
		{
			clicked?.Invoke();
		}

		private void Show(bool shown)
		{
			if (Native.IsConnected)
			{
				Native.Api->UiVisible(Id, shown ? 1 : 0);
			}
		}

		private bool ChangeClass(string name, int change)
		{
			if (!Native.IsConnected)
			{
				return false;
			}

			var bytes = Encoding.UTF8.GetBytes(name);
			fixed (byte* text = bytes)
			{
				return Native.Api->UiClass(Id, text, bytes.Length, change) != 0;
			}
		}

		private string ReadText()
		{
			if (!Native.IsConnected)
			{
				return string.Empty;
			}

			var length = Native.Api->UiText(Id, null, 0);
			if (length <= 0)
			{
				return string.Empty;
			}
			var buffer = new byte[length];
			fixed (byte* into = buffer)
			{
				Native.Api->UiText(Id, into, buffer.Length);
			}
			return Encoding.UTF8.GetString(buffer);
		}

		private void WriteText(string text)
		{
			if (!Native.IsConnected)
			{
				return;
			}

			var bytes = Encoding.UTF8.GetBytes(text);
			fixed (byte* written = bytes)
			{
				Native.Api->UiSetText(Id, written, bytes.Length);
			}
		}
	}
}
