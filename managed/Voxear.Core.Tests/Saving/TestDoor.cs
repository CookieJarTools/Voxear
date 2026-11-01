namespace Voxear.Tests
{
	internal sealed class TestDoor : Behaviour
	{
		[Saved] public bool IsOpen;
		public float Width = 1;

		[Saved] private int knocks;

		[Saved]
		private float Swing { get; set; }

		[Saved]
		public int Hinges => 2;

		public int Knocks => knocks;

		public float SwingNow => Swing;

		public void Knock(float swing)
		{
			knocks++;
			Swing = swing;
		}
	}
}
