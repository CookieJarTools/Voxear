using System;

namespace Voxear
{
	/// <summary>Runs a system in another stage of the frame than Update: <c>[UpdateIn(Stage.FixedUpdate)]</c>.</summary>
	[AttributeUsage(AttributeTargets.Class, Inherited = true)]
	public sealed class UpdateInAttribute : Attribute
	{
		public UpdateInAttribute(Stage stage)
		{
			Stage = stage;
		}

		public Stage Stage { get; }
	}
}
