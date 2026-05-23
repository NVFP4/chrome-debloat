use std::env;
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::Command;

use windows_registry::LOCAL_MACHINE;

const ELEVATION_ATTEMPTED_ARG: &str = "--chrome-debloat-elevation-attempted";
const SYSTEM_POLICY_ROOT: &str = r"SOFTWARE\Policies";

pub fn relaunch_elevated_if_needed() -> bool {
    if elevation_was_already_attempted() {
        return false;
    }
    if !needs_elevation() {
        return false;
    }

    let Ok(exe) = env::current_exe() else {
        return false;
    };
    let Ok(status) = relaunch_command(&exe, env::args_os().skip(1)).status() else {
        return false;
    };

    status.success()
}

fn elevation_was_already_attempted() -> bool {
    env::args_os().any(|arg| arg == OsStr::new(ELEVATION_ATTEMPTED_ARG))
}

pub fn needs_elevation() -> bool {
    !is_elevated()
}

pub fn clipboard_text() -> Option<String> {
    clipboard_win::get_clipboard_string()
        .ok()
        .filter(|text| !text.is_empty())
}

fn is_elevated() -> bool {
    LOCAL_MACHINE
        .options()
        .write()
        .open(SYSTEM_POLICY_ROOT)
        .is_ok()
}

fn relaunch_command<I>(exe: &Path, args: I) -> Command
where
    I: IntoIterator<Item = OsString>,
{
    let mut command = powershell();
    command.arg("-Command").arg(relaunch_script(exe, args));
    command
}

fn relaunch_script<I>(exe: &Path, args: I) -> String
where
    I: IntoIterator<Item = OsString>,
{
    let exe = powershell_string(&exe.as_os_str().to_string_lossy());
    let mut args = args
        .into_iter()
        .filter(|arg| arg != OsStr::new(ELEVATION_ATTEMPTED_ARG))
        .collect::<Vec<_>>();
    args.push(OsString::from(ELEVATION_ATTEMPTED_ARG));

    let args = args
        .into_iter()
        .map(|arg| powershell_string(&arg.to_string_lossy()))
        .collect::<Vec<_>>();
    let argument_list = format!(" -ArgumentList @({})", args.join(", "));

    format!(
        "$ErrorActionPreference = 'Stop'; \
         Start-Process -FilePath {}{} -Verb RunAs -Wait",
        exe, argument_list,
    )
}

fn powershell_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn powershell() -> Command {
    let mut command = Command::new("powershell.exe");
    command.args([
        "-NoProfile",
        "-Sta",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
    ]);
    command
}
