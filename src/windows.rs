use std::env;
use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Stdio};

const ADMIN_CHECK_SCRIPT: &str = concat!(
    "$identity = [Security.Principal.WindowsIdentity]::GetCurrent();",
    "$principal = [Security.Principal.WindowsPrincipal]::new($identity);",
    "if ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {",
    "exit 0",
    "} else {",
    "exit 1",
    "}",
);

const RELAUNCH_SCRIPT: &str = concat!(
    "$exe = $args[0];",
    "if ($args.Count -gt 1) {",
    "$appArgs = $args[1..($args.Count - 1)];",
    "Start-Process -FilePath $exe -ArgumentList $appArgs -Verb RunAs -Wait",
    "} else {",
    "Start-Process -FilePath $exe -Verb RunAs -Wait",
    "}",
);

pub fn relaunch_elevated_if_needed() -> bool {
    if !needs_elevation() {
        return false;
    }

    eprintln!("Requesting administrator permissions...");
    let Ok(exe) = env::current_exe() else {
        return false;
    };
    let Ok(status) = relaunch_command(&exe, env::args_os().skip(1)).status() else {
        return false;
    };

    status.success()
}

pub fn needs_elevation() -> bool {
    !is_elevated()
}

fn is_elevated() -> bool {
    powershell()
        .arg("-Command")
        .arg(ADMIN_CHECK_SCRIPT)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn relaunch_command<I>(exe: &Path, args: I) -> Command
where
    I: IntoIterator<Item = OsString>,
{
    let mut command = powershell();
    command
        .arg("-Command")
        .arg(RELAUNCH_SCRIPT)
        .arg(exe)
        .args(args);
    command
}

fn powershell() -> Command {
    let mut command = Command::new("powershell.exe");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
    ]);
    command
}
