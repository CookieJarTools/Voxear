using System;

namespace Voxear
{
	/// <summary>
	/// Makes a static field or property a console variable, read and set by name in a development build's console:
	/// <c>[ConsoleVariable("Takes no damage")] public static bool God;</c>, then <c>God</c> to see it and
	/// <c>God true</c> to set it. It takes text, a toggle, a number, an enum's name, or a vector as "x,y,z".
	/// </summary>
	[AttributeUsage(AttributeTargets.Field | AttributeTargets.Property)]
	public sealed class ConsoleVariableAttribute : Attribute
	{
		public ConsoleVariableAttribute(string help = "")
		{
			Help = help;
		}

		/// <summary>What the console's help says it is for.</summary>
		public string Help { get; }
	}
}
