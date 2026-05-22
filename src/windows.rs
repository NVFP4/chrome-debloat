use std::env;
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::{Command, Stdio};

const ELEVATION_ATTEMPTED_ARG: &str = "--chrome-debloat-elevation-attempted";
const ADMIN_CHECK_SCRIPT: &str = concat!(
    "$identity = [Security.Principal.WindowsIdentity]::GetCurrent();",
    "$principal = New-Object Security.Principal.WindowsPrincipal $identity;",
    "if ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {",
    "exit 0",
    "} else {",
    "exit 1",
    "}",
);
const CLIPBOARD_SCRIPT: &str = concat!(
    "$ErrorActionPreference = 'Stop';",
    "$OutputEncoding = [System.Text.UTF8Encoding]::new($false);",
    "[Console]::OutputEncoding = $OutputEncoding;",
    "$value = Get-Clipboard -Raw -Format Text;",
    "if ($null -ne $value) { [Console]::Out.Write($value) }",
);

pub fn relaunch_elevated_if_needed() -> bool {
    if elevation_was_already_attempted() {
        return false;
    }
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

fn elevation_was_already_attempted() -> bool {
    env::args_os().any(|arg| arg == OsStr::new(ELEVATION_ATTEMPTED_ARG))
}

pub fn needs_elevation() -> bool {
    !is_elevated()
}

pub fn clipboard_text() -> Option<String> {
    let output = powershell()
        .arg("-Command")
        .arg(CLIPBOARD_SCRIPT)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let text = match String::from_utf8(output.stdout) {
        Ok(text) => text,
        Err(error) => String::from_utf8_lossy(&error.into_bytes()).into_owned(),
    };

    (!text.is_empty()).then_some(text)
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
