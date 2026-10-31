//! A stack frame as the Console shows it: a line of a .NET stack trace, a
//! Rust log's file and line, or the C# compiler's place for an error,
//! parsed into the parts it is coloured by.
//!
//! A trace's shape varies (a log's stack, an exception's own trace, async
//! state machines), so, as Sifter learned, nothing is matched against one
//! shape: the place comes off the end, the parameters are found by counting
//! brackets back from there, and whatever is in front is the type and the
//! method. A line that does not parse keeps its text and is shown as it is.

/// One frame, in parts. A part the line did not have is empty.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frame {
    /// The line as it was written.
    pub raw: String,
    pub namespace: String,
    pub class: String,
    pub method: String,
    pub parameters: String,
    pub file: Option<String>,
    pub line: Option<u32>,
}

impl Frame {
    /// The assembly a frame is from, as the Console can hide them: its root
    /// namespace (`Voxear`, `System`, the game's), or for the engine's own
    /// frames, its crate (`voxear_engine`). `None` when it cannot tell.
    pub fn assembly(&self) -> Option<&str> {
        if let Some(root) = self.namespace.split('.').next().filter(|root| !root.is_empty()) {
            return Some(root);
        }
        let file = self.file.as_deref()?;
        file.strip_prefix("crates/").and_then(|rest| rest.split('/').next())
    }

    /// A frame of a .NET stack trace: `at Hello.Spinner.Update() in
    /// /path/Spinner.cs:line 42`. `None` for a line that is no frame, such
    /// as the note between an async method's parts.
    pub fn parse(line: &str) -> Option<Frame> {
        let raw = line.trim();
        if raw.is_empty() || raw.starts_with("---") {
            return None;
        }
        let text = raw.strip_prefix("at ").unwrap_or(raw);
        let (call, place) = match text.rsplit_once(" in ") {
            Some((call, place)) => (call.trim(), Some(place.trim())),
            None => (text, None),
        };
        let mut frame = Frame { raw: raw.to_string(), ..Frame::default() };
        if let Some(place) = place {
            let (file, line) = place.rsplit_once(":line ").unwrap_or((place, ""));
            frame.file = Some(file.to_string());
            frame.line = line.trim().parse().ok();
        }
        frame.read_call(call);
        Some(frame)
    }

    /// Where a Rust log was made.
    pub fn at(file: &str, line: Option<u32>) -> Frame {
        let raw = match line {
            Some(line) => format!("{file}:{line}"),
            None => file.to_string(),
        };
        Frame { raw, file: Some(file.to_string()), line, ..Frame::default() }
    }

    /// The place the C# compiler gives an error or warning:
    /// `/path/Spinner.cs(12,5): error CS1002: ; expected`.
    pub fn compiler(message: &str) -> Option<Frame> {
        let (place, _) = message.split_once("): ")?;
        let (file, at) = place.rsplit_once('(')?;
        let line = at.split(',').next()?.trim().parse().ok()?;
        Some(Frame {
            raw: format!("{file}:{line}"),
            file: Some(file.to_string()),
            line: Some(line),
            ..Frame::default()
        })
    }

    /// Whether the type and method were found, so the parts can be shown.
    pub fn has_call(&self) -> bool {
        !self.method.is_empty()
    }

    /// The file's name, without its folders.
    pub fn file_name(&self) -> Option<&str> {
        let file = self.file.as_deref()?;
        Some(file.rsplit(['/', '\\']).next().unwrap_or(file))
    }

    /// `Namespace.Class.Method(parameters)`: the parameters by counting
    /// brackets back from the end, the method after the last dot before
    /// them, and the class after the dot before that.
    fn read_call(&mut self, call: &str) {
        let Some(open) = opening_bracket(call) else { return };
        self.parameters = call[open + 1..call.len() - 1].to_string();
        let path = &call[..open];
        // A constructor is `Class..ctor`: its method starts with the second dot.
        let split = match path.rfind("..") {
            Some(at) if path[at..].starts_with("..ctor") || path[at..].starts_with("..cctor") => Some(at),
            _ => path.rfind('.'),
        };
        let Some(split) = split else {
            self.method = path.to_string();
            return;
        };
        self.method = path[split + 1..].to_string();
        let owner = &path[..split];
        match owner.rsplit_once('.') {
            Some((namespace, class)) => (self.namespace, self.class) = (namespace.to_string(), class.to_string()),
            None => self.class = owner.to_string(),
        }
        self.tidy_async();
    }

    /// An async method's body is a state machine, `<Start>d__5.MoveNext`, in
    /// the class that declares it: shown as that class's `Start`, as written.
    fn tidy_async(&mut self) {
        let named = self.class.strip_prefix('<').and_then(|c| c.split_once(">d__")).map(|(name, _)| name.to_string());
        let (Some(name), "MoveNext") = (named, self.method.as_str()) else { return };
        let (namespace, owner) = self.namespace.rsplit_once('.').unwrap_or(("", &self.namespace));
        (self.class, self.method) = (owner.to_string(), format!("{name} (async)"));
        self.namespace = namespace.to_string();
    }
}

/// Where the parameter list opens: the bracket matching the one the call
/// ends with, counted back so brackets inside parameter types still match.
fn opening_bracket(call: &str) -> Option<usize> {
    if !call.ends_with(')') {
        return None;
    }
    let mut depth = 0;
    for (at, c) in call.char_indices().rev() {
        match c {
            ')' => depth += 1,
            '(' => {
                depth -= 1;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_read_in_every_shape_dotnet_writes() {
        let log = Frame::parse("   at Hello.Spinner.Update() in /home/jeo/Hello/Scripts/Spinner.cs:line 42").unwrap();
        assert_eq!((log.namespace.as_str(), log.class.as_str(), log.method.as_str()), ("Hello", "Spinner", "Update"));
        assert_eq!((log.file.as_deref(), log.line), (Some("/home/jeo/Hello/Scripts/Spinner.cs"), Some(42)));
        assert_eq!(log.file_name(), Some("Spinner.cs"));

        let args =
            Frame::parse("at Voxear.Scripting.FieldValues.Read(JsonElement value, Type type, String path)").unwrap();
        assert_eq!(args.parameters, "JsonElement value, Type type, String path");
        assert_eq!((args.namespace.as_str(), args.class.as_str()), ("Voxear.Scripting", "FieldValues"));
        assert_eq!(args.file, None);

        let generic = Frame::parse("at System.Collections.Generic.List`1.get_Item(Int32 index)").unwrap();
        assert_eq!((generic.class.as_str(), generic.method.as_str()), ("List`1", "get_Item"));

        let built = Frame::parse("at Hello.Pulse..ctor() in /p/Pulse.cs:line 9").unwrap();
        assert_eq!((built.class.as_str(), built.method.as_str()), ("Pulse", ".ctor"));

        let awaited = Frame::parse("at Hello.GameManager.<Start>d__5.MoveNext() in /p/GameManager.cs:line 30").unwrap();
        assert_eq!((awaited.namespace.as_str(), awaited.class.as_str()), ("Hello", "GameManager"));
        assert_eq!(awaited.method, "Start (async)");

        let nested = Frame::parse("at Hello.Tools.Swap(Func`2 pick (Obj, Boolean)) in /p/Tools.cs:line 3").unwrap();
        assert_eq!(nested.parameters, "Func`2 pick (Obj, Boolean)", "brackets inside the parameters still match");
        assert_eq!(nested.method, "Swap");

        assert_eq!(Frame::parse("--- End of stack trace from previous location ---"), None);
        let odd = Frame::parse("at <unknown>").unwrap();
        assert!(!odd.has_call() && odd.raw == "at <unknown>", "kept as it was");
    }

    #[test]
    fn compiler_errors_and_rust_logs_give_their_place() {
        let error =
            Frame::compiler("/home/jeo/Hello/Scripts/Spinner.cs(12,5): error CS1002: ; expected [/x.csproj]").unwrap();
        assert_eq!((error.file.as_deref(), error.line), (Some("/home/jeo/Hello/Scripts/Spinner.cs"), Some(12)));
        assert_eq!(Frame::compiler("Build FAILED."), None);
        let rust = Frame::at("crates/voxear_engine/src/levels.rs", Some(120));
        assert_eq!((rust.file_name(), rust.line), (Some("levels.rs"), Some(120)));
    }
}
