//! Child-process plumbing for `codex app-server`.

use crate::error::Error;
use crate::events::Flags;
use tokio::process::Child;
use tokio::process::ChildStderr;
use tokio::process::ChildStdin;
use tokio::process::ChildStdout;
use tokio::process::Command;

/// Spawns the backend with piped stdio and the same environment as the GUI.
///
/// `kill_on_drop` guarantees no orphaned backend when the GUI runtime shuts
/// down mid-flight. The `take` calls are invariants of the piped-stdio
/// configuration set two lines above.
#[allow(clippy::expect_used)]
pub async fn spawn(flags: &Flags) -> Result<(Child, ChildStdin, ChildStdout, ChildStderr), Error> {
    let mut command = Command::new(&flags.program);
    command
        .arg("app-server")
        .args(&flags.args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    let mut child = command.spawn().map_err(|source| Error::Spawn {
        program: flags.program.display().to_string(),
        source,
    })?;

    let stdin = child
        .stdin
        .take()
        .expect("stdin is piped by spawn configuration");
    let stdout = child
        .stdout
        .take()
        .expect("stdout is piped by spawn configuration");
    let stderr = child
        .stderr
        .take()
        .expect("stderr is piped by spawn configuration");

    Ok((child, stdin, stdout, stderr))
}
