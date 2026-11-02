namespace Voxear;

/// <summary>
/// A key by where it is on the keyboard, as a US keyboard labels it, whatever this keyboard prints on it: W is the
/// key above S, so WASD keeps its shape on AZERTY. Bindings name keys the same way, such as <c>Keyboard/W</c>.
/// </summary>
public enum Key
{
	A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
	Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
	Space, Enter, Escape, Tab, Backspace, Delete, Insert, Home, End, PageUp, PageDown,
	ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
	ShiftLeft, ShiftRight, ControlLeft, ControlRight, AltLeft, AltRight, SuperLeft, SuperRight, CapsLock,
	Backquote, Minus, Equal, BracketLeft, BracketRight, Backslash, Semicolon, Quote, Comma, Period, Slash,
	F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
	Numpad0, Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, Numpad8, Numpad9,
	NumpadAdd, NumpadSubtract, NumpadMultiply, NumpadDivide, NumpadDecimal, NumpadEnter,
}
