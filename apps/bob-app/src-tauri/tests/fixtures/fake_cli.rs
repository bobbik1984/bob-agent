// Process-control fixture only. Never calls any service.
use std::{io::{Read,Write},time::Duration,process::{Command,Stdio}};
fn main(){
 if std::env::args().nth(1).as_deref()==Some("--sleep-child"){std::thread::sleep(Duration::from_secs(60));return}
 let mut input=String::new();std::io::stdin().read_to_string(&mut input).unwrap();
 if input.contains("fixture-fail"){std::process::exit(7)}
 if input.contains("fixture-overflow"){let mut out=std::io::stdout();for _ in 0..1024{out.write_all(&[b'x';8192]).unwrap();}return}
 if input.contains("fixture-slow") || input.contains("fixture-cancel") {
  let child=Command::new(std::env::current_exe().unwrap()).arg("--sleep-child").stdin(Stdio::null()).spawn().unwrap();
  std::fs::write("fixture-child.pid",child.id().to_string()).unwrap();
  std::thread::sleep(Duration::from_secs(60));return
 }
 println!("{{\"type\":\"thread.started\",\"thread_id\":\"fixture\"}}");
 println!("{{\"type\":\"turn.started\"}}");
 println!("{{\"type\":\"item.completed\",\"item\":{{\"id\":\"1\",\"type\":\"agent_message\",\"text\":\"FIXTURE_REPLY\"}}}}");
 println!("{{\"type\":\"turn.completed\"}}");
}
