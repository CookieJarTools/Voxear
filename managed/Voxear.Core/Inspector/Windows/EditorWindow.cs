namespace Voxear
{
	/// <summary>
	/// A window of the editor's, written in your game's C#, as Unity's EditorWindow: a panel of the dock, opened from
	/// any window's menu as many times as you like, each its own instance. It is built with Voxear UI, as a game's UI
	/// and drawers are: add its elements to the root in Build and keep them in fields, or describe them in Markup.
	/// Update runs every frame it shows, while you edit and while the game plays.
	/// <code>
	/// public sealed class LevelTools : EditorWindow
	/// {
	///     private Label count = null!;
	///
	///     protected override void Build(UiElement root)
	///     {
	///         count = root.Add(new Label());
	///         root.Add(new Button("Swap Spinners")).Clicked += Tools.SwapSpinners;
	///     }
	///
	///     protected override void Update(UiDocument ui)
	///     {
	///         count.Text = $"{Obj.Find("Spinner A").Name} is here";
	///     }
	/// }
	/// </code>
	/// <para>
	/// The menu calls it by its class name in words, Level Tools; [Window] gives it a title and an icon of your own.
	/// </para>
	/// </summary>
	public abstract class EditorWindow
	{
		private UiElement? root;

		/// <summary>
		/// The elements it shows, as a .vui document holds them: <c>&lt;vui&gt;…&lt;/vui&gt;</c>, with the sheets its Style
		/// elements name read from the project. Leave it out to build the elements in code, in Build.
		/// </summary>
		public virtual string Markup => string.Empty;

		/// <summary>Adds its elements to `root`, once, before its first Update. Keep them in fields to use in Update.</summary>
		protected virtual void Build(UiElement root)
		{
		}

		/// <summary>
		/// Runs every frame the window shows, after the Clicked and Changed of its elements, to keep what it shows up
		/// to date. `ui` finds the elements its markup named.
		/// </summary>
		protected virtual void Update(UiDocument ui)
		{
		}

		/// <summary>Runs as the window closes, or as the scripts reload, to let go of what it holds.</summary>
		protected virtual void Closed()
		{
		}

		/// <summary>The markup the editor builds, an empty document when it builds in code.</summary>
		internal string MarkupToBuild => Markup.Length > 0 ? Markup : "<vui></vui>";

		/// <summary>Builds it the first time, and raises what was clicked or changed in what it built, then updates it.</summary>
		internal void Run(UiDocument ui)
		{
			if (root == null)
			{
				root = ui.Root;
				Build(root);
			}
			else
			{
				root.DispatchTree();
			}
			Update(ui);
		}

		internal void Close()
		{
			Closed();
		}
	}
}
