//! Authenticated native DSH protocol; never forwards arbitrary methods.
use serde_json::{json, Value};
use std::{io::{Read, Write}, time::{Duration, Instant}, net::{TcpStream, ToSocketAddrs}};
use tungstenite::{client::IntoClientRequest, Message, stream::MaybeTlsStream};
use crate::dsh_session::{History, SessionAddress, project_history};
const LIMIT: u64 = 4 * 1024 * 1024;
// History snapshots include tool records as well as visible messages. Keep a separate,
// bounded budget; a size rejection must not masquerade as an offline node.
const HISTORY_LIMIT: usize = 16 * 1024 * 1024;
fn rpc_transport_error(method:&str, timeout:bool)-> &'static str {
 if method=="session/prompt" {"DSH_RECEIPT_UNKNOWN"}
 else if method.starts_with("workspaceFiles/") {if timeout {"DSH_FILE_TIMEOUT"}else{"DSH_FILE_UNAVAILABLE"}}
 else {if timeout {"DSH_HISTORY_TIMEOUT"}else{"DSH_HISTORY_UNAVAILABLE"}}
}
fn history_read_error(error: tungstenite::Error) -> &'static str {
 match error {
  tungstenite::Error::Capacity(_) => "DSH_HISTORY_TOO_LARGE",
  tungstenite::Error::Io(e) if matches!(e.kind(),std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock) => "DSH_HISTORY_TIMEOUT",
  _ => "DSH_HISTORY_UNAVAILABLE",
 }
}
// Only server-read history can authorize a preview. Scope includes credential identity;
// pages are bounded and volatile, never client-supplied grants or persistent history copies.
struct PreviewReference { scope: String, session: String, root: String, text: String, at: Instant }
#[derive(Default)]
struct PreviewReferences { entries: std::collections::VecDeque<PreviewReference>, bytes: usize }
impl PreviewReferences {
 fn remember(&mut self, scope: &str, h: &History) {
  let Some(root)=h.workspace.as_ref() else {return};
  let text=h.messages.iter().filter(|m|m.role=="assistant").map(|m|m.text.as_str()).collect::<Vec<_>>().join("\n");
  if text.is_empty() || text.len()>1024*1024 {return;}
  self.entries.retain(|e|e.at.elapsed()<Duration::from_secs(600) && !(e.scope==scope && e.session==h.address.session_id && (e.root!=*root || e.text==text)));
  self.bytes=self.entries.iter().map(|e|e.text.len()).sum();
  while self.entries.len()>=64 || self.bytes+text.len()>8*1024*1024 {if let Some(e)=self.entries.pop_front(){self.bytes-=e.text.len()}else{break}}
  self.bytes+=text.len();
  self.entries.push_back(PreviewReference{scope:scope.into(),session:h.address.session_id.clone(),root:root.clone(),text,at:Instant::now()});
 }
 fn root(&self,scope:&str,session:&str,path:&str)->Option<String>{
  self.entries.iter().rev().find(|e|e.scope==scope && e.session==session && e.at.elapsed()<Duration::from_secs(600) && e.text.contains(path)).map(|e|e.root.clone())
 }
}
fn preview_references()-> &'static std::sync::Mutex<PreviewReferences>{
 static CACHE:std::sync::OnceLock<std::sync::Mutex<PreviewReferences>>=std::sync::OnceLock::new();
 CACHE.get_or_init(||std::sync::Mutex::new(PreviewReferences::default()))
}
pub struct DshClient { base: reqwest::Url, cookie: String, http: reqwest::blocking::Client, scope: String }
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
  use sha2::{Digest,Sha256};
  let scope=format!("{:x}",Sha256::digest(format!("{}\0{}",base,token).as_bytes()));
  Ok(Self{base,cookie:cookies[0].into(),http,scope})
 }
 fn rpc(&self, method:&str,args:Value)->Result<Value,&'static str>{
  let id=format!("bob-{}-{}",std::process::id(),crate::now_millis());
  let response=self.http.post(self.base.join(&format!("api/{method}")).map_err(|_|"DSH_PROTOCOL")?).header("Cookie",&self.cookie).header("Origin",self.base.origin().ascii_serialization()).json(&json!({"type":"client-request","rpcId":id,"method":method,"payload":{"args":args}})).send().map_err(|e|rpc_transport_error(method,e.is_timeout()))?;
  if !response.status().is_success(){return Err("DSH_REJECTED");}
  let limit=if method=="session/page" {HISTORY_LIMIT as u64}else{LIMIT};
  let mut bytes=Vec::new();response.take(limit+1).read_to_end(&mut bytes).map_err(|e|rpc_transport_error(method,matches!(e.kind(),std::io::ErrorKind::TimedOut|std::io::ErrorKind::WouldBlock)))?;
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
  let h=project_history(address.clone(),&self.snapshot(&address,200)?)?;
  preview_references().lock().map_err(|_|"DSH_UNAVAILABLE")?.remember(&self.scope,&h);
  Ok(h)
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
  if after.is_some_and(|n|n< -1) || after.is_some() && before.is_some(){return Err("DSH_PAGE_CURSOR_INVALID");}
  // Obtain the log cut without subscribing to a multi-megabyte follow snapshot.
  let projection=self.rpc("session/projections",json!({"request":{"sessionId":address.session_id}}))?;
  if projection["kind"]!="sequenced" {return Err("DSH_PROTOCOL");}
  let current=projection["asOfSeq"].as_i64().filter(|n|*n>= -1).ok_or("DSH_PROTOCOL")?;
  if after==Some(current){return Ok(json!({"address":address,"cursor":current,"not_modified":true,"messages":[],"before":0,"source_requests":1,"source_bytes":serde_json::to_vec(&projection).map_err(|_|"DSH_PROTOCOL")?.len()}));}
  let started=Instant::now();
  let cursor=through.unwrap_or(current);
  if cursor>current {return Err("DSH_PAGE_CURSOR_INVALID");}
  let page=self.rpc("session/page",json!({"request":{"address":{"kind":"session","sessionId":address.session_id},"throughSeq":cursor,"beforeSeq":before.unwrap_or(cursor.checked_add(1).ok_or("DSH_PAGE_CURSOR_INVALID")?),"maxMessages":64}}))?;
  let mut snapshot=json!({"type":"snapshot","header":{"id":address.session_id},"projections":projection,"cursor":cursor,"records":page["records"],"hasMore":page["hasMore"]});
  let mut requests=2;
  let mut bytes=serde_json::to_vec(&snapshot).map_err(|_|"DSH_PROTOCOL")?.len();
  loop {
   let h=project_history(address.clone(),&snapshot)?;
   let records=snapshot["records"].as_array().ok_or("DSH_PROTOCOL")?;
   let first=records.first().map(|v|v["event"]["seq"].as_i64().ok_or("DSH_PROTOCOL")).transpose()?.unwrap_or(0);
   if records.iter().any(|v|v["event"]["seq"].as_i64().is_none_or(|n|n<0||n>cursor||before.is_some_and(|b|n>=b))) {return Err("DSH_PAGE_CURSOR_INVALID");}
   if h.messages.len()>=10 || !h.partial || first==0 || requests>=32 || started.elapsed()>Duration::from_secs(20) {
    preview_references().lock().map_err(|_|"DSH_UNAVAILABLE")?.remember(&self.scope,&h);
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
   let page=self.rpc("session/page",json!({"request":{"address":{"kind":"session","sessionId":address.session_id},"throughSeq":cursor,"beforeSeq":first,"maxMessages":64}}))?;
   bytes+=serde_json::to_vec(&page).map_err(|_|"DSH_PROTOCOL")?.len();requests+=1;
   if bytes>HISTORY_LIMIT{return Err("DSH_HISTORY_TOO_LARGE");}
   let mut older=page["records"].as_array().ok_or("DSH_PROTOCOL")?.clone();
   if older.is_empty() && page["hasMore"]==true {return Err("DSH_PAGE_NO_PROGRESS");}
   if older.iter().any(|v|v["event"]["seq"].as_i64().is_none_or(|n|n<0||n>=first)){return Err("DSH_PAGE_CURSOR_INVALID");}
   older.extend(records.clone());snapshot["records"]=json!(older);snapshot["hasMore"]=page["hasMore"].clone();
  }
 }
 pub fn preview(&self,address:SessionAddress,path:&str)->Result<Value,&'static str>{self.preview_if_changed(address,path,None)}
 pub fn preview_if_changed(&self,address:SessionAddress,path:&str,known:Option<&Value>)->Result<Value,&'static str>{
  let ext=preview_extension(path)?;
  let cached=preview_references().lock().map_err(|_|"DSH_UNAVAILABLE")?.root(&self.scope,&address.session_id,path);
  let root=match cached {
   Some(root)=>root,
   None=>{
    self.history_page(address.clone(),None,None,None)?;
    preview_references().lock().map_err(|_|"DSH_UNAVAILABLE")?.root(&self.scope,&address.session_id,path).ok_or("DSH_FILE_NOT_REFERENCED")?
   }
  };
  let expected=format!("{}/{}",root.trim_end_matches('/'),path);
  let args=json!({"workspaceFileScopeId":address.session_id,"path":path});
  let stat=self.rpc("workspaceFiles/stat",args.clone())?;
  if stat["absolutePath"]!=expected {return Err("DSH_FILE_OUTSIDE_WORKSPACE");}
  let size=stat["bytes"].as_u64().filter(|n|*n<=2*1024*1024).ok_or("DSH_FILE_TOO_LARGE")?;
  if known.is_some_and(|k| !stat["version"].is_null() && k["version"]==stat["version"] && k["bytes"]==stat["bytes"]) {
   return Ok(json!({"not_modified":true,"path":path,"connection_id":address.connection_id,"session_id":address.session_id,"version":stat["version"],"bytes":size}));
  }
  let mut url=self.base.join("api/file").map_err(|_|"DSH_PROTOCOL")?;
  url.query_pairs_mut().append_pair("path",&expected);
  let response=self.http.get(url).header("Cookie",&self.cookie).timeout(Duration::from_secs(30)).send().map_err(|e|if e.is_timeout(){"DSH_FILE_TIMEOUT"}else{"DSH_FILE_UNAVAILABLE"})?;
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
  let client=DshClient{base:reqwest::Url::parse("https://example.invalid/").unwrap(),cookie:String::new(),scope:String::new(),http:reqwest::blocking::Client::new()};
  let address=SessionAddress{connection_id:"c".into(),session_id:"s".into()};
  for (before,through,after) in [(Some(-1),Some(2),None),(Some(1),None,None),(None,Some(2),None),(None,None,Some(-2)),(Some(1),Some(2),Some(2))] {
   assert_eq!(client.history_page(address.clone(),before,through,after).unwrap_err(),"DSH_PAGE_CURSOR_INVALID");
  }
 }
}

#[cfg(test)] mod reference_cache_tests {
 use super::*;
 fn history(session:&str,root:&str,text:&str)->History {
  History{address:SessionAddress{connection_id:"connection".into(),session_id:session.into()},workspace:Some(root.into()),messages:vec![crate::dsh_session::Message{sequence:1,role:"assistant".into(),text:text.into()}],partial:true,omitted:0,request_results:vec![]}
 }
 #[test] fn read_timeout_is_not_an_unknown_send_receipt(){
  assert_eq!(rpc_transport_error("session/page",true),"DSH_HISTORY_TIMEOUT");
  assert_eq!(rpc_transport_error("workspaceFiles/stat",true),"DSH_FILE_TIMEOUT");
  assert_eq!(rpc_transport_error("session/prompt",true),"DSH_RECEIPT_UNKNOWN");
 }
 #[test] fn references_are_scoped_and_require_server_history() {
  let mut cache=PreviewReferences::default();
  let mut h=history("s","/workspace","![image](snapshots/a.png)");
  cache.remember("auth-a",&h);
  assert_eq!(cache.root("auth-a","s","snapshots/a.png"),Some("/workspace".into()));
  assert_eq!(cache.root("auth-b","s","snapshots/a.png"),None);
  assert_eq!(cache.root("auth-a","other","snapshots/a.png"),None);
  assert_eq!(cache.root("auth-a","s","private.txt"),None);
  h.messages[0].role="user".into();cache.remember("auth-b",&h);
  assert_eq!(cache.root("auth-b","s","snapshots/a.png"),None);
 }
 #[test] fn references_expire_and_workspace_change_invalidates_old_scope() {
  let mut cache=PreviewReferences::default();
  cache.remember("a",&history("s","/old","a.png"));
  cache.entries[0].at=Instant::now()-Duration::from_secs(601);
  assert_eq!(cache.root("a","s","a.png"),None);
  cache.remember("a",&history("s","/old","a.png"));
  cache.remember("a",&history("s","/new","b.png"));
  assert_eq!(cache.root("a","s","a.png"),None);
  assert_eq!(cache.root("a","s","b.png"),Some("/new".into()));
 }
 #[test] fn reference_storage_is_bounded_and_preserves_multiple_pages() {
  let mut cache=PreviewReferences::default();
  for i in 0..100 {cache.remember("a",&history(&format!("s{i}"),"/workspace",&"x".repeat(200000)));}
  assert!(cache.entries.len()<=64);assert!(cache.bytes<=8*1024*1024);
  cache.remember("a",&history("s","/workspace","older.png"));
  cache.remember("a",&history("s","/workspace","newer.png"));
  assert!(cache.root("a","s","older.png").is_some());
  assert!(cache.root("a","s","newer.png").is_some());
 }
}
