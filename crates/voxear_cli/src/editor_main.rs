//! `voxear-editor`: the editor on its own, as the launcher starts it, with
//! `--project <folder>` (or the folder alone). It hands over to `vox editor`
//! beside it, so an install carries the engine once.

use std::{ffi::OsString, process::Command};

fn main() {
    let mut args = std::env::args_os().skip(1);
    let mut forwarded: Vec<OsString> = Vec::new();
    while let Some(arg) = args.next() {
        // The project is `vox editor`'s positional argument.
        if arg == "--project" {
            forwarded.extend(args.next());
        } else {
            forwarded.push(arg);
        }
    }
    let vox = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join(format!("vox{}", std::env::consts::EXE_SUFFIX))))
        .unwrap_or_else(|| "vox".into());
    let mut command = Command::new(&vox);
    command.arg("editor").args(forwarded);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = command.exec();
        eprintln!("could not start {}: {error}", vox.display());
        std::process::exit(1);
    }
    #[cfg(not(unix))]
    match command.status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(error) => {
            eprintln!("could not start {}: {error}", vox.display());
            std::process::exit(1);
        }
    }
}
