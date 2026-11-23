// Read-only connection diagnostics. Credential is read from stdin, never arguments/logs.
use std::{io::{self,Read},time::Instant};
use bob_runtime::{dsh_transport::DshClient,dsh_session::SessionAddress};
fn main(){if let Err(e)=run(){eprintln!("{}",e);std::process::exit(1)}}
fn run()->Result<(), &'static str>{
 let args:Vec<_>=std::env::args().collect();if args.len()!=4{return Err("usage: dsh_read_probe endpoint session relative-path (credential on stdin)")}
 let mut token=String::new();io::stdin().read_to_string(&mut token).map_err(|_|"credential input failed")?;
 let start=Instant::now();let client=DshClient::connect(&args[1],token.trim())?;drop(token);
 println!("{}",serde_json::json!({"segment":"authentication","ms":start.elapsed().as_millis()}));
 let address=SessionAddress{connection_id:"diagnostic".into(),session_id:args[2].clone()};
 let start=Instant::now();let page=client.history_page(address.clone(),None,None,None)?;
 println!("{}",serde_json::json!({"segment":"page","ms":start.elapsed().as_millis(),"messages":page["messages"].as_array().map(Vec::len),"source_bytes":page["source_bytes"],"source_requests":page["source_requests"],"cursor":page["cursor"],"before":page["before"],"has_more":page["has_more"]}));
 let start=Instant::now();let file=client.preview(address,&args[3])?;
 println!("{}",serde_json::json!({"segment":"file","ms":start.elapsed().as_millis(),"bytes":file["bytes"],"sha256":file["sha256"]}));Ok(())
}
