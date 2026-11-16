//! The code editors scripts and the project open in, and how each is
//! started on a file at a line. Whisk comes first and is the default: the
//! Voxear Launcher installs it, and it opens the project folder with the
//! file, so its C# help knows the whole project.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize};

/// A code editor the Code editor setting can name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CodeEditor {
    #[default]
    Whisk,
    VsCode,
    Rider,
    Zed,
    /// Whatever the desktop opens the file with, without its line.
    SystemDefault,
}

impl CodeEditor {
    pub const ALL: [CodeEditor; 5] =
        [CodeEditor::Whisk, CodeEditor::VsCode, CodeEditor::Rider, CodeEditor::Zed, CodeEditor::SystemDefault];

    pub const fn name(self) -> &'static str {
        match self {
            CodeEditor::Whisk => "Whisk",
            CodeEditor::VsCode => "VS Code",
            CodeEditor::Rider => "Rider",
            CodeEditor::Zed => "Zed",
            CodeEditor::SystemDefault => "System default",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|editor| *editor == self).unwrap_or(0)
    }

    pub fn at(index: usize) -> Self {
        Self::ALL[index.min(Self::ALL.len() - 1)]
    }

    /// The program and arguments that open `folder`, the project, and
    /// `file` at `line` in this editor, or the folder alone. `None` for Whisk
    /// when it is not installed.
    pub fn command(self, folder: &Path, file: Option<(&Path, u32)>) -> Option<(OsString, Vec<OsString>)> {
        let os = |text: &str| OsString::from(text);
        let at = |path: &Path, line: u32| {
            let mut arg = path.as_os_str().to_owned();
            arg.push(format!(":{}", line.max(1)));
            arg
        };
        let folder = folder.as_os_str().to_owned();
        Some(match (self, file) {
            // Whisk takes the folder, then each file with its line.
            (CodeEditor::Whisk, file) => {
                let mut args = vec![folder];
                args.extend(file.map(|(path, line)| at(path, line)));
                (find_whisk()?.into_os_string(), args)
            }
            (CodeEditor::VsCode, Some((path, line))) => (os("code"), vec![folder, os("--goto"), at(path, line)]),
            (CodeEditor::VsCode, None) => (os("code"), vec![folder]),
            (CodeEditor::Rider, Some((path, line))) => {
                (os("rider"), vec![os("--line"), os(&line.to_string()), path.as_os_str().to_owned()])
            }
            (CodeEditor::Rider, None) => (os("rider"), vec![folder]),
            (CodeEditor::Zed, Some((path, line))) => (os("zed"), vec![folder, at(path, line)]),
            (CodeEditor::Zed, None) => (os("zed"), vec![folder]),
            (CodeEditor::SystemDefault, file) => {
                let target = file.map_or(folder, |(path, _)| path.as_os_str().to_owned());
                (os(desktop_opener()), vec![target])
            }
        })
    }
}

/// What opens a file as the desktop does.
pub fn desktop_opener() -> &'static str {
    if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    }
}

/// Reads the setting by name, or by its place in the list as projects
/// saved it before Whisk was on it.
impl<'de> Deserialize<'de> for CodeEditor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Saved {
            Name(String),
            Index(usize),
        }
        Ok(match Saved::deserialize(deserializer)? {
            Saved::Name(name) => match name.as_str() {
                "whisk" => CodeEditor::Whisk,
                "vs-code" => CodeEditor::VsCode,
                "rider" => CodeEditor::Rider,
                "zed" => CodeEditor::Zed,
                "system-default" => CodeEditor::SystemDefault,
                other => {
                    return Err(serde::de::Error::unknown_variant(
                        other,
                        &["whisk", "vs-code", "rider", "zed", "system-default"],
                    ));
                }
            },
            // The list was VS Code, Rider, Zed and System default.
            Saved::Index(index) => {
                [CodeEditor::VsCode, CodeEditor::Rider, CodeEditor::Zed, CodeEditor::SystemDefault][index.min(3)]
            }
        })
    }
}

/// Where Whisk is: the Voxear Launcher's copy, then where Whisk last said it
/// was, then where its packages and installer put it, then the path.
pub fn find_whisk() -> Option<PathBuf> {
    launcher_copy()
        .filter(|path| path.is_file())
        .or_else(recorded_whisk)
        .or_else(|| installed_whisk())
        .or_else(|| on_path(if cfg!(windows) { "whisk.exe" } else { "whisk" }))
}

/// The Voxear Launcher keeps Whisk in its own folder on Linux:
/// `<launcher folder>/tools/whisk/Whisk.AppImage`.
fn launcher_copy() -> Option<PathBuf> {
    let root = match std::env::var_os("VOXUP_HOME").filter(|root| !root.is_empty()) {
        Some(root) => PathBuf::from(root),
        None if cfg!(windows) => PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Voxear"),
        None => data_home()?.join("voxear"),
    };
    Some(root.join("tools").join("whisk").join("Whisk.AppImage"))
}

fn data_home() -> Option<PathBuf> {
    match std::env::var_os("XDG_DATA_HOME").filter(|home| !home.is_empty()) {
        Some(home) => Some(PathBuf::from(home)),
        None => Some(home()?.join(".local").join("share")),
    }
}

fn home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}

/// Whisk writes where it is to `~/.whisk/install-path` each time it starts
/// on Linux. When that file has gone, as an AppImage updated under a new
/// name does, another Whisk AppImage in its folder is taken.
fn recorded_whisk() -> Option<PathBuf> {
    let text = std::fs::read_to_string(home()?.join(".whisk").join("install-path")).ok()?;
    let path = PathBuf::from(text.trim());
    if path.is_file() {
        return Some(path);
    }
    let mut siblings: Vec<PathBuf> = std::fs::read_dir(path.parent()?)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| is_whisk_appimage(path) && path.is_file())
        .collect();
    siblings.sort();
    siblings.pop()
}

fn is_whisk_appimage(path: &Path) -> bool {
    let name = path.file_name().map(|name| name.to_string_lossy().to_lowercase()).unwrap_or_default();
    name.starts_with("whisk") && name.ends_with(".appimage")
}

/// Whisk's `.deb` and `.rpm` put it in `/usr/bin`; its Windows installer in
/// `%LOCALAPPDATA%\Whisk`.
fn installed_whisk() -> Option<PathBuf> {
    let path = if cfg!(windows) {
        PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Whisk").join("whisk.exe")
    } else {
        PathBuf::from("/usr/bin/whisk")
    };
    path.is_file().then_some(path)
}

fn on_path(program: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?).map(|folder| folder.join(program)).find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whisk_comes_first_and_is_the_default() {
        assert_eq!(CodeEditor::default(), CodeEditor::Whisk);
        assert_eq!(CodeEditor::ALL[0], CodeEditor::Whisk);
        for editor in CodeEditor::ALL {
            assert_eq!(CodeEditor::at(editor.index()), editor);
        }
        assert_eq!(CodeEditor::at(99), CodeEditor::SystemDefault);
    }

    #[test]
    fn reads_names_and_the_old_numbers() {
        #[derive(Deserialize)]
        struct Saved {
            code_editor: CodeEditor,
        }
        let read = |text: &str| toml::from_str::<Saved>(text).map(|saved| saved.code_editor);
        assert_eq!(read("code_editor = \"whisk\"").unwrap(), CodeEditor::Whisk);
        assert_eq!(read("code_editor = \"system-default\"").unwrap(), CodeEditor::SystemDefault);
        // Before Whisk: VS Code, Rider, Zed, System default.
        assert_eq!(read("code_editor = 0").unwrap(), CodeEditor::VsCode);
        assert_eq!(read("code_editor = 2").unwrap(), CodeEditor::Zed);
        assert!(read("code_editor = \"notepad\"").is_err());
        let written =
            toml::to_string(&std::collections::BTreeMap::from([("code_editor", CodeEditor::VsCode)])).unwrap();
        assert_eq!(written.trim(), "code_editor = \"vs-code\"");
    }

    #[test]
    fn opens_the_file_at_its_line_beside_the_project() {
        let folder = Path::new("/games/Hello");
        let file = Path::new("/games/Hello/Scripts/Spinner.cs");
        let (program, args) = CodeEditor::VsCode.command(folder, Some((file, 33))).unwrap();
        assert_eq!(program, "code");
        assert_eq!(args, ["/games/Hello", "--goto", "/games/Hello/Scripts/Spinner.cs:33"]);
        let (program, args) = CodeEditor::Rider.command(folder, Some((file, 33))).unwrap();
        assert_eq!((program.to_str().unwrap(), args.len()), ("rider", 3));
        let (_, args) = CodeEditor::SystemDefault.command(folder, Some((file, 33))).unwrap();
        assert_eq!(args, [file.as_os_str()], "the desktop opener takes no line");
        let (_, args) = CodeEditor::Zed.command(folder, None).unwrap();
        assert_eq!(args, [folder.as_os_str()]);
    }

    #[test]
    fn whisk_takes_the_folder_then_the_file_at_its_line() {
        let folder = Path::new("/games/Hello");
        let file = Path::new("/games/Hello/Scripts/Spinner.cs");
        // Only where Whisk is installed; elsewhere there is nothing to start.
        match CodeEditor::Whisk.command(folder, Some((file, 0))) {
            Some((program, args)) => {
                assert_eq!(Some(PathBuf::from(&program)), find_whisk());
                assert_eq!(args, ["/games/Hello", "/games/Hello/Scripts/Spinner.cs:1"], "lines start at 1");
            }
            None => assert_eq!(find_whisk(), None),
        }
    }
}
