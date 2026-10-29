//! Manual integration replay only. Never starts a model or marks Home delivery.
use bob_kernel::app::LocalApp;
fn main() -> Result<(),Box<dyn std::error::Error>> {
 let args:Vec<_>=std::env::args_os().collect();
 if args.len()!=3 {return Err("usage: replay_cli_probe DATABASE EVENTS_JSONL".into())}
 let stream=std::fs::read_to_string(&args[2])?;
 let mut app=LocalApp::open(std::path::Path::new(&args[1]))?;
 let session=app.default_session()?;
 let request="approved-independent-cli-probe-20261029";
 let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis() as i64;
 app.accept(request,"independent-cli-probe-message",&session,
  "Independent CLI test: reply exactly BOB_REPLY_OK 2+3=5. Imported for persistence/UI verification; not dispatched from Bob.".into(),now)?;
 app.record_cli_probe(request,&stream,Some(0),now+1)?;
 println!("CAPTURED_REPLY_REPLAYED_NOT_DISPATCHED");
 Ok(())
}
