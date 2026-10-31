//! Authenticated native DSH protocol; never forwards arbitrary methods.
use serde_json::{json, Value};
use std::{io::{Read, Write}, time::{Duration, Instant}, net::{TcpStream, ToSocketAddrs}};
use tungstenite::{client::IntoClientRequest, Message, stream::MaybeTlsStream};
use crate::dsh_session::{History, SessionAddress, project_history};
const LIMIT: u64 = 4 * 1024 * 1024;
// History snapshots include tool records as well as visible messages. Keep a separate,
// bounded budget; a size rejection must not masquerade as an offline node.
const HISTORY_LIMIT: usize = 16 * 1024 * 1024;
fn history_read_error(error: tungstenite::Error) -> &'static str {
 match error {
  tungstenite::Error::Capacity(_) => "DSH_HISTORY_TOO_LARGE",
  tungstenite::Error::Io(e) if matches!(e.kind(),std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock) => "DSH_HISTORY_TIMEOUT",
  _ => "DSH_HISTORY_UNAVAILABLE",
 }
}
pub struct DshClient { base: reqwest::Url, cookie: String, http: reqwest::blocking::Client }
impl DshClient {
 pub fn connect(endpoint: &str, token: &str) -> Result<Self, &'static str> {
  let mut base=reqwest::Url::parse(endpoint).map_err(|_|"DSH_ADDRESS_INVALID")?;
  if base.scheme()!="https" || !base.username().is_empty() || base.password().is_some() || base.query().is_some() || base.fragment().is_some() || base.path()!="/" { return Err("DSH_HTTPS_REQUIRED"); }
  let http=reqwest::blocking::Client::builder().user_agent("Bob/1.0.1").redirect(reqwest::redirect::Policy::none()).timeout(Duration::from_secs(15)).build().map_err(|_|"DSH_UNAVAILABLE")?;
  base.query_pairs_mut().append_pair("token",token);
  let response=http.get(base.clone()).send().map_err(|_|"DSH_UNAVAILABLE")?;
  base.set_query(None);
  if response.status().as_u16()!=303 {return Err("DSH_AUTH_REJECTED");}
  let location=response.headers().get("location").and_then(|v|v.to_str().ok()).ok_or("DSH_PROTOCOL")?;
  if base.join(location).map_err(|_|"DSH_PROTOCOL")?!=base{return Err("DSH_PROTOCOL");}
  let cookies:Vec<_>=response.headers().get_all("set-cookie").iter().filter_map(|v|v.to_str().ok()).filter_map(|s|s.split(';').next()).filter(|s|s.starts_with("dsh-auth-")).collect();
  if cookies.len()!=1 || cookies[0].len()>8192{return Err("DSH_PROTOCOL");}
  Ok(Self{base,cookie:cookies[0].into(),http})
 }
 fn rpc(&self, method:&str,args:Value)->Result<Value,&'static str>{
  let id=format!("bob-{}-{}",std::process::id(),crate::now_millis());
  let response=self.http.post(self.base.join(&format!("api/{method}")).map_err(|_|"DSH_PROTOCOL")?).header("Cookie",&self.cookie).header("Origin",self.base.origin().ascii_serialization()).json(&json!({"type":"client-request","rpcId":id,"method":method,"payload":{"args":args}})).send().map_err(|_|"DSH_RECEIPT_UNKNOWN")?;
  if !response.status().is_success(){return Err("DSH_REJECTED");}
  let mut bytes=Vec::new();response.take(LIMIT+1).read_to_end(&mut bytes).map_err(|_|"DSH_RECEIPT_UNKNOWN")?;
  if bytes.len() as u64>LIMIT{return Err("DSH_PROTOCOL");}
  let value:Value=serde_json::from_slice(&bytes).map_err(|_|"DSH_PROTOCOL")?;
  if value["type"]!="server-response" || value["rpcId"]!=id || value["result"]["ok"]!=true{return Err("DSH_REJECTED");}
  Ok(value["result"]["value"].clone())
 }
 pub fn list(&self)->Result<Value,&'static str>{
  let value=self.rpc("session/list",json!({"_request":{}}))?;
  let items=value["items"].as_array().ok_or("DSH_PROTOCOL")?;
  if items.len()>5000{return Err("DSH_PROTOCOL");}
  let mut output=Vec::new();
  for s in items.iter().filter(|s|s["origin"]!="subagent") {
   let id=s["sessionId"].as_str().ok_or("DSH_PROTOCOL")?;
   let running=s["running"].as_bool().ok_or("DSH_PROTOCOL")?;
   output.push(json!({"id":id,"title":s["projections"]["values"]["title"].as_str().unwrap_or(id),"updated_at":s["updatedAt"],"running":running,"workspace":s["projections"]["values"]["workingDirectory"],"source_kind":"dsh_native"}));
  }
  output.sort_by_key(|s|std::cmp::Reverse(s["updated_at"].as_i64().unwrap_or(0)));
  Ok(json!(output))
 }
 pub fn history(&self,address:SessionAddress)->Result<History,&'static str>{
  let mut url=self.base.join("api/remote.mux").map_err(|_|"DSH_PROTOCOL")?;url.set_scheme("wss").map_err(|_|"DSH_PROTOCOL")?;
  let mut req=url.as_str().into_client_request().map_err(|_|"DSH_PROTOCOL")?;
  req.headers_mut().insert("Cookie",self.cookie.parse().map_err(|_|"DSH_PROTOCOL")?);
  req.headers_mut().insert("Origin",self.base.origin().ascii_serialization().parse().map_err(|_|"DSH_PROTOCOL")?);
  req.headers_mut().insert("User-Agent","Bob/1.0.1".parse().unwrap());
  let host=self.base.host_str().ok_or("DSH_ADDRESS_INVALID")?;
  let addresses=(host,self.base.port_or_known_default().ok_or("DSH_ADDRESS_INVALID")?).to_socket_addrs().map_err(|_|"DSH_UNAVAILABLE")?;
  let mut socket=None;
  for address in addresses.take(4) { if let Ok(s)=TcpStream::connect_timeout(&address,Duration::from_secs(3)){socket=Some(s);break;} }
  let socket=socket.ok_or("DSH_UNAVAILABLE")?;
  socket.set_read_timeout(Some(Duration::from_secs(10))).map_err(|_|"DSH_UNAVAILABLE")?;
  socket.set_write_timeout(Some(Duration::from_secs(10))).map_err(|_|"DSH_UNAVAILABLE")?;
  let config=tungstenite::protocol::WebSocketConfig::default().max_message_size(Some(HISTORY_LIMIT)).max_frame_size(Some(HISTORY_LIMIT));
  let (mut ws,_)=tungstenite::client_tls_with_config(req,socket,Some(config),None).map_err(|_|"DSH_UNAVAILABLE")?;
  match ws.get_mut(){MaybeTlsStream::Rustls(s)=>{s.sock.set_read_timeout(Some(Duration::from_secs(15))).map_err(|_|"DSH_UNAVAILABLE")?;s.sock.set_write_timeout(Some(Duration::from_secs(15))).map_err(|_|"DSH_UNAVAILABLE")?;},_=>return Err("DSH_TLS_REQUIRED")}
  let stream=format!("bob-{}",crate::now_millis());
  ws.send(Message::Text(json!({"type":"open","streamId":stream,"endpoint":"session/follow","payload":{"args":{"request":{"address":{"kind":"session","sessionId":address.session_id},"maxMessages":200}}}}).to_string().into())).map_err(|_|"DSH_UNAVAILABLE")?;
  let mut size=0; let started=Instant::now();
  for _ in 0..100 {
   if started.elapsed()>Duration::from_secs(20){return Err("DSH_HISTORY_TIMEOUT");}
   let item=ws.read().map_err(history_read_error)?;
   if let Message::Text(text)=item {
    size+=text.len();if size>HISTORY_LIMIT{return Err("DSH_HISTORY_TOO_LARGE");}
    let value:Value=serde_json::from_str(&text).map_err(|_|"DSH_PROTOCOL")?;
    if value["streamId"]==stream && value["type"]=="item" && value["value"]["type"]=="snapshot"{
     let result=project_history(address,&value["value"]);
     let _=ws.send(Message::Text(json!({"type":"cancel","streamId":stream}).to_string().into()));
     let _=ws.close(None);return result;
    }
   }
  }Err("DSH_HISTORY_UNAVAILABLE")
 }
 pub fn prompt(&self,session:&str,text:&str,id:&str)->Result<Value,&'static str>{
  let result=self.rpc("session/prompt",json!({"request":{"requestId":id,"sessionId":session,"mode":"queue","content":[{"type":"text","text":text}]}}))?;
  if result["accepted"]!=true{return Err("DSH_RECEIPT_UNKNOWN");} Ok(json!({"status":"accepted","session_id":session,"request_id":id}))
 }
}

// Durable one-shot dispatch receipts. A crash or ambiguous response never permits replay.
pub fn dispatch_once(dir:&std::path::Path, connection:&str, session:&str, text:&str, id:&str, send:impl FnOnce()->Result<Value,&'static str>)->Result<Value,&'static str>{
 use sha2::{Digest,Sha256};
 let receipts=dir.join("dsh-dispatch-receipts");
 std::fs::create_dir_all(&receipts).map_err(|_|"DSH_RECEIPT_WRITE_FAILED")?;
 let key=format!("{:x}",Sha256::digest(serde_json::to_vec(&(connection,session,id)).map_err(|_|"DSH_PROTOCOL")?));
 let path=receipts.join(format!("{key}.json"));
 let digest=format!("{:x}",Sha256::digest(text.as_bytes()));
 match std::fs::OpenOptions::new().write(true).create_new(true).open(&path){
  Ok(mut file)=>{file.write_all(json!({"status":"unknown","text_sha256":digest}).to_string().as_bytes()).and_then(|_|file.sync_all()).map_err(|_|"DSH_RECEIPT_WRITE_FAILED")?;},
  Err(e) if e.kind()==std::io::ErrorKind::AlreadyExists=>{
   let previous:Value=serde_json::from_slice(&std::fs::read(&path).map_err(|_|"DSH_RECEIPT_UNKNOWN")?).map_err(|_|"DSH_RECEIPT_UNKNOWN")?;
   if previous["text_sha256"]!=digest{return Err("DSH_REQUEST_CONFLICT");}
   return if previous["status"]=="accepted"{Ok(previous["receipt"].clone())}else{Err("DSH_RECEIPT_UNKNOWN")};
  },Err(_)=>return Err("DSH_RECEIPT_WRITE_FAILED")
 }
 let result=send()?;
 let mut file=std::fs::OpenOptions::new().write(true).truncate(true).open(path).map_err(|_|"DSH_RECEIPT_UNKNOWN")?;
 file.write_all(json!({"status":"accepted","text_sha256":digest,"receipt":result}).to_string().as_bytes()).and_then(|_|file.sync_all()).map_err(|_|"DSH_RECEIPT_UNKNOWN")?;
 Ok(result)
}
#[cfg(test)] mod tests{
 use super::*;
 #[test] fn history_capacity_and_timeout_have_distinct_errors(){
  assert_eq!(history_read_error(tungstenite::Error::Capacity(tungstenite::error::CapacityError::MessageTooLong{size:20*1024*1024,max_size:HISTORY_LIMIT})),"DSH_HISTORY_TOO_LARGE");
  assert_eq!(history_read_error(tungstenite::Error::Io(std::io::Error::from(std::io::ErrorKind::TimedOut))),"DSH_HISTORY_TIMEOUT");
  assert_eq!(history_read_error(tungstenite::Error::ConnectionClosed),"DSH_HISTORY_UNAVAILABLE");
 }
 #[test] fn real_sized_snapshot_fits_bounded_history_budget(){
  use tungstenite::{WebSocket,protocol::Role};
  let payload="x".repeat(4_684_655);
  let mut sender=WebSocket::from_raw_socket(std::io::Cursor::new(Vec::new()),Role::Server,None);
  sender.send(Message::Text(payload.clone().into())).unwrap();
  let bytes=sender.get_ref().get_ref().clone();
  let config=tungstenite::protocol::WebSocketConfig::default().max_message_size(Some(HISTORY_LIMIT)).max_frame_size(Some(HISTORY_LIMIT));
  let mut receiver=WebSocket::from_raw_socket(std::io::Cursor::new(bytes),Role::Client,Some(config));
  assert_eq!(receiver.read().unwrap().into_text().unwrap().len(),payload.len());
 }
 #[test] fn refuses_replay_after_unknown(){let d=tempfile::tempdir().unwrap();assert!(dispatch_once(d.path(),"c","s","hello","r",||Err("DSH_RECEIPT_UNKNOWN")).is_err());assert!(dispatch_once(d.path(),"c","s","hello","r",||panic!("must not resend")).is_err());}
 #[test] fn dedup_and_bind_payload(){let d=tempfile::tempdir().unwrap();let v=json!({"status":"accepted"});assert_eq!(dispatch_once(d.path(),"c","s","hello","r",||Ok(v.clone())).unwrap(),v);assert_eq!(dispatch_once(d.path(),"c","s","hello","r",||panic!("duplicate")).unwrap(),v);assert_eq!(dispatch_once(d.path(),"c","s","changed","r",||panic!("conflict")).unwrap_err(),"DSH_REQUEST_CONFLICT");}
}
