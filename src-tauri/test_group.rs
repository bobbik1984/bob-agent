
use command_group::AsyncCommandGroup;
use tokio::process::Command;

#[tokio::main]
async fn main() {
    let mut cmd = Command::new("cmd");
    cmd.arg("/c").arg("echo hi");
    let mut child = cmd.group_spawn().unwrap();
    let stdin = child.inner().stdin.take();
    child.kill().await.unwrap();
}
