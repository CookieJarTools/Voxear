namespace Voxear.Tests
{
	internal sealed class TestMover : Behaviour
	{
		[FoldoutGroup("Movement")] [Range(0, 20)] [Suffix("m/s")] [Tooltip("How fast it walks.")]
		[SerializeField] private float speed = 6;

		[FoldoutGroup("Movement")] [EnumToggleButtons]
		[SerializeField] private MoveMode mode;

		[FoldoutGroup("Movement")] [ShowIf(nameof(mode), MoveMode.Fly)] [MinValue(0)]
		[SerializeField] private float liftForce;

		[Required] [InfoBox("Plays when it is hit.")] [InfoBox("Nothing plays.", InfoMessageType.Warning, nameof(mode))]
		[SerializeField] private Prefab hitEffect;

		[HideInInspector] [SerializeField] private int secret;

		[PropertyOrder(-1)] [LabelText("Display name")] [Multiline(2)]
		[SerializeField] private string title = "";
	}
}
