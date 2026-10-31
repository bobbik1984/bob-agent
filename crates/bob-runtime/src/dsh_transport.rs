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
  let limit=if method=="session/page" {HISTORY_LIMIT as u64}else{LIMIT};
  let mut bytes=Vec::new();response.take(limit+1).read_to_end(&mut bytes).map_err(|_|"DSH_RECEIPT_UNKNOWN")?;
  if bytes.len() as u64>limit{return Err("DSH_PROTOCOL");}
  let value:Value=serde_json::from_slice(&bytes).map_err(|_|"DSH_PROTOCOL")?;
  if value["type"]!="server-response" || value["rpcId"]!=id {return Err("DSH_PROTOCOL");}
  if value["result"]["ok"]!=true {
   if method.starts_with("workspaceFiles/") { eprintln!("DSH preview rejected: {}", value["result"]["error"]["code"].as_str().unwrap_or("unknown")); }
   return Err("DSH_REJECTED");
  }
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
  project_history(address.clone(),&self.snapshot(&address,200)?)
 }
 fn snapshot(&self,address:&SessionAddress,max_messages:usize)->Result<Value,&'static str>{
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
  ws.send(Message::Text(json!({"type":"open","streamId":stream,"endpoint":"session/follow","payload":{"args":{"request":{"address":{"kind":"session","sessionId":address.session_id},"maxMessages":max_messages}}}}).to_string().into())).map_err(|_|"DSH_UNAVAILABLE")?;
  let mut size=0; let started=Instant::now();
  for _ in 0..100 {
   if started.elapsed()>Duration::from_secs(20){return Err("DSH_HISTORY_TIMEOUT");}
   let item=ws.read().map_err(history_read_error)?;
   if let Message::Text(text)=item {
    size+=text.len();if size>HISTORY_LIMIT{return Err("DSH_HISTORY_TOO_LARGE");}
    let value:Value=serde_json::from_str(&text).map_err(|_|"DSH_PROTOCOL")?;
    if value["streamId"]==stream && value["type"]=="item" && value["value"]["type"]=="snapshot"{
     let result=Ok(value["value"].clone());
     let _=ws.send(Message::Text(json!({"type":"cancel","streamId":stream}).to_string().into()));
     let _=ws.close(None);return result;
    }
   }
  }Err("DSH_HISTORY_UNAVAILABLE")
 }
 /// Native bounded history paging; cursor is an inclusive immutable log cut.
 pub fn history_page(&self,address:SessionAddress,before:Option<i64>,through:Option<i64>,after:Option<i64>)->Result<Value,&'static str>{
  if before.is_some_and(|n|n<0) || through.is_some_and(|n|n< -1) || before.is_some()!=through.is_some(){return Err("DSH_PAGE_CURSOR_INVALID");}
  if let Some(after)=after {
   if after< -1 || before.is_some(){return Err("DSH_PAGE_CURSOR_INVALID");}
   let projection=self.rpc("session/projections",json!({"request":{"sessionId":address.session_id}}))?;
   if projection["kind"]=="sequenced" && projection["asOfSeq"].as_i64()==Some(after){return Ok(json!({"address":address,"cursor":after,"not_modified":true,"messages":[],"before":0,"source_requests":1,"source_bytes":serde_json::to_vec(&projection).map_err(|_|"DSH_PROTOCOL")?.len()}));}
  }
  let started=Instant::now();
  let mut snapshot=if let (Some(b),Some(c))=(before,through) {
   let page=self.rpc("session/page",json!({"request":{"address":{"kind":"session","sessionId":address.session_id},"throughSeq":c,"beforeSeq":b,"maxMessages":100}}))?;
   json!({"type":"snapshot","header":{"id":address.session_id},"cursor":c,"records":page["records"],"hasMore":page["hasMore"]})
  }else{self.snapshot(&address,100)?};
  let cursor=snapshot["cursor"].as_i64().filter(|n|*n>= -1).ok_or("DSH_PROTOCOL")?;
  let mut requests=1;
  let mut bytes=serde_json::to_vec(&snapshot).map_err(|_|"DSH_PROTOCOL")?.len();
  loop {
   let h=project_history(address.clone(),&snapshot)?;
   let records=snapshot["records"].as_array().ok_or("DSH_PROTOCOL")?;
   let first=records.first().map(|v|v["event"]["seq"].as_i64().ok_or("DSH_PROTOCOL")).transpose()?.unwrap_or(0);
   if records.iter().any(|v|v["event"]["seq"].as_i64().is_none_or(|n|n<0||n>cursor||before.is_some_and(|b|n>=b))) {return Err("DSH_PAGE_CURSOR_INVALID");}
   if h.messages.len()>=10 || !h.partial || first==0 || requests>=32 || started.elapsed()>Duration::from_secs(20) {
    let mut output=serde_json::to_value(&h).map_err(|_|"DSH_PROTOCOL")?;
    let skip=h.messages.len().saturating_sub(10);
    output["messages"]=json!(&h.messages[skip..]);
    output["cursor"]=json!(cursor);
    output["before"]=json!(if skip>0 {h.messages[skip].sequence} else {first});
    output["has_more"]=json!(h.partial || skip>0);
    output["source_requests"]=json!(requests);
    output["source_bytes"]=json!(bytes);
    output["replacements"]=json!(records.iter().filter_map(|v|{let op=&v["event"]["surfaceOp"];if op.is_object(){Some(op.clone())}else{None}}).collect::<Vec<_>>());
    return Ok(output);
   }
   let page=self.rpc("session/page",json!({"request":{"address":{"kind":"session","sessionId":address.session_id},"throughSeq":cursor,"beforeSeq":first,"maxMessages":100}}))?;
   bytes+=serde_json::to_vec(&page).map_err(|_|"DSH_PROTOCOL")?.len();requests+=1;
   if bytes>HISTORY_LIMIT{return Err("DSH_HISTORY_TOO_LARGE");}
   let mut older=page["records"].as_array().ok_or("DSH_PROTOCOL")?.clone();
   if older.is_empty() && page["hasMore"]==true {return Err("DSH_PAGE_NO_PROGRESS");}
   if older.iter().any(|v|v["event"]["seq"].as_i64().is_none_or(|n|n<0||n>=first)){return Err("DSH_PAGE_CURSOR_INVALID");}
   older.extend(records.clone());snapshot["records"]=json!(older);snapshot["hasMore"]=page["hasMore"].clone();
  }
 }
 pub fn preview(&self,address:SessionAddress,path:&str)->Result<Value,&'static str>{
  let ext=preview_extension(path)?;
  let h=self.history(address.clone())?;
  if !h.messages.iter().any(|m|m.role=="assistant" && m.text.contains(path)){return Err("DSH_FILE_NOT_REFERENCED");}
  let root=h.workspace.ok_or("DSH_FILE_SCOPE_MISSING")?;
  let expected=format!("{}/{}",root.trim_end_matches('/'),path);
  let args=json!({"workspaceFileScopeId":address.session_id,"path":path});
  let stat=self.rpc("workspaceFiles/stat",args.clone())?;
  if stat["absolutePath"]!=expected {return Err("DSH_FILE_OUTSIDE_WORKSPACE");}
  let size=stat["bytes"].as_u64().filter(|n|*n<=2*1024*1024).ok_or("DSH_FILE_TOO_LARGE")?;
  let mut url=self.base.join("api/file").map_err(|_|"DSH_PROTOCOL")?;
  url.query_pairs_mut().append_pair("path",&expected);
  let response=self.http.get(url).header("Cookie",&self.cookie).send().map_err(|_|"DSH_FILE_UNAVAILABLE")?;
  if !response.status().is_success(){return Err(match response.status().as_u16(){404=>"DSH_FILE_NOT_FOUND",413=>"DSH_FILE_TOO_LARGE",_=>"DSH_FILE_UNAVAILABLE"});}
  let mut bytes=Vec::new();
  response.take(2*1024*1024+1).read_to_end(&mut bytes).map_err(|_|"DSH_FILE_UNAVAILABLE")?;
  if bytes.len() as u64!=size {return Err("DSH_FILE_CHANGED");}
  let after=self.rpc("workspaceFiles/stat",args)?;
  if after["absolutePath"]!=stat["absolutePath"] || after["version"]!=stat["version"] || after["bytes"]!=stat["bytes"] {return Err("DSH_FILE_CHANGED");}
  use base64::Engine;
  use sha2::{Digest,Sha256};
  Ok(json!({"path":path,"extension":ext,"data":base64::engine::general_purpose::STANDARD.encode(&bytes),"bytes":size,"version":stat["version"],"sha256":format!("{:x}",Sha256::digest(&bytes)),"source":"current_workspace","connection_id":address.connection_id,"session_id":address.session_id}))
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

// Read-only recovery: absence or a matching message is not proof of non-delivery.
pub fn inspect_receipt(dir:&std::path::Path, connection:&str, session:&str, text:&str, id:&str)->Result<Value,&'static str>{
 use sha2::{Digest,Sha256};
 let key=format!("{:x}",Sha256::digest(serde_json::to_vec(&(connection,session,id)).map_err(|_|"DSH_PROTOCOL")?));
 let path=dir.join("dsh-dispatch-receipts").join(format!("{key}.json"));
 let bytes=match std::fs::read(path){Ok(b)=>b,Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(json!({"status":"unknown"})),Err(_)=>return Err("DSH_RECEIPT_UNKNOWN")};
 let value:Value=serde_json::from_slice(&bytes).map_err(|_|"DSH_RECEIPT_UNKNOWN")?;
 let digest=format!("{:x}",Sha256::digest(text.as_bytes()));
 if value["text_sha256"]!=digest{return Err("DSH_REQUEST_CONFLICT");}
 if value["status"]=="accepted" && value["receipt"]["session_id"]==session && value["receipt"]["request_id"]==id {
  Ok(value["receipt"].clone())
 }else{Ok(json!({"status":"unknown"}))}
}
#[cfg(test)] mod recovery_tests {
 use super::*;
 #[test] fn recovery_requires_exact_identity_and_payload(){
  let d=tempfile::tempdir().unwrap();
  let receipt=json!({"status":"accepted","session_id":"s","request_id":"r"});
  dispatch_once(d.path(),"c","s","hello","r",||Ok(receipt.clone())).unwrap();
  assert_eq!(inspect_receipt(d.path(),"c","s","hello","r").unwrap(),receipt);
  assert_eq!(inspect_receipt(d.path(),"other","s","hello","r").unwrap()["status"],"unknown");
  assert_eq!(inspect_receipt(d.path(),"c","s","changed","r").unwrap_err(),"DSH_REQUEST_CONFLICT");
 }
 #[test] fn uncertain_receipt_never_unlocks_resend(){
  let d=tempfile::tempdir().unwrap();
  let _=dispatch_once(d.path(),"c","s","hello","r",||Err("DSH_RECEIPT_UNKNOWN"));
  assert_eq!(inspect_receipt(d.path(),"c","s","hello","r").unwrap()["status"],"unknown");
  assert!(dispatch_once(d.path(),"c","s","hello","r",||panic!("replayed")).is_err());
 }
}

fn preview_extension(path:&str)->Result<String,&'static str>{
  if path.is_empty() || path.len()>512 || path.contains(['\\',':','%','?','#']) || path.starts_with('/') || path.split('/').any(|p|p.is_empty()||p=="."||p==".."||p.starts_with('.')) {return Err("DSH_FILE_PATH_REJECTED");}
  let ext=path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
  if !["png","jpg","jpeg","webp","md","txt","json","yaml","yml","gd","tscn","js","css","html"].contains(&ext.as_str()){return Err("DSH_FILE_TYPE_UNSUPPORTED");}
  Ok(ext)
}

#[cfg(test)] mod preview_tests {
 use super::*;
 #[test] fn allowed_preview_paths(){assert_eq!(preview_extension("snapshots/view.png").unwrap(),"png");assert_eq!(preview_extension("docs/progress.md").unwrap(),"md");}
 #[test] fn reject_unsafe_preview_paths(){for p in ["../secret.md","/etc/config.txt","a/../../b.md",".env","a/.secret.txt","C:/secret.txt","a%2fb.md","https://host/a.png","private.key", "a//b.md"] {assert!(preview_extension(p).is_err(),"{p}");}}
}

#[cfg(test)] mod paging_tests {
 use super::*;
 #[test] fn invalid_page_cursors_fail_before_network() {
  let client=DshClient{base:reqwest::Url::parse("https://example.invalid/").unwrap(),cookie:String::new(),http:reqwest::blocking::Client::new()};
  let address=SessionAddress{connection_id:"c".into(),session_id:"s".into()};
  for (before,through,after) in [(Some(-1),Some(2),None),(Some(1),None,None),(None,Some(2),None),(None,None,Some(-2)),(Some(1),Some(2),Some(2))] {
   assert_eq!(client.history_page(address.clone(),before,through,after).unwrap_err(),"DSH_PAGE_CURSOR_INVALID");
  }
 }
}
