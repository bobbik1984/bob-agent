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
 fn session_root(&self,scope:&str,session:&str)->Option<String>{self.entries.iter().rev().find(|e|e.scope==scope && e.session==session).map(|e|e.root.clone())}
 fn root(&self,scope:&str,session:&str,path:&str)->Option<String>{
  self.entries.iter().rev().find(|e|e.scope==scope && e.session==session && e.at.elapsed()<Duration::from_secs(600) && e.text.contains(path)).map(|e|e.root.clone())
 }
}
fn preview_references()-> &'static std::sync::Mutex<PreviewReferences>{
 static CACHE:std::sync::OnceLock<std::sync::Mutex<PreviewReferences>>=std::sync::OnceLock::new();
 CACHE.get_or_init(||std::sync::Mutex::new(PreviewReferences::default()))
}
fn shared_http_client() -> Result<reqwest::blocking::Client, &'static str> {
 static HTTP: std::sync::OnceLock<reqwest::blocking::Client> = std::sync::OnceLock::new();
 if let Some(c) = HTTP.get() {
  return Ok(c.clone());
 }
 let built = reqwest::blocking::Client::builder()
  .user_agent("Bob/1.0.1")
  .redirect(reqwest::redirect::Policy::none())
  .local_address(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))
  .connect_timeout(Duration::from_secs(10))
  .timeout(Duration::from_secs(20))
  .build()
  .map_err(|_| "DSH_UNAVAILABLE")?;
 let _ = HTTP.set(built.clone());
 Ok(built)
}
// Plain HTTP is permitted only for a literal loopback service on this host.
// Remote devices must use TLS; DNS names cannot opt into this exception.
fn trusted_endpoint(base: &reqwest::Url) -> bool {
 let loopback = base.host_str().and_then(|h|h.trim_matches(['[',']']).parse::<std::net::IpAddr>().ok()).is_some_and(|ip|ip.is_loopback());
 (base.scheme()=="https" || (base.scheme()=="http" && loopback)) && base.username().is_empty() && base.password().is_none() && base.query().is_none() && base.fragment().is_none() && base.path()=="/"
}
#[derive(Clone)]
pub struct DshClient { base: reqwest::Url, cookie: String, http: reqwest::blocking::Client, scope: String }
impl DshClient {
 pub fn connect(endpoint: &str, token: &str) -> Result<Self, &'static str> {
  let mut base=reqwest::Url::parse(endpoint).map_err(|_|"DSH_ADDRESS_INVALID")?;
  if !trusted_endpoint(&base) { return Err("DSH_HTTPS_REQUIRED"); }
  let http=shared_http_client()?;
  base.query_pairs_mut().append_pair("token",token);
  let response=http.get(base.clone()).send().map_err(|_|"DSH_UNAVAILABLE")?;
  base.set_query(None);
  if response.status().as_u16()!=303 {return Err("DSH_AUTH_REJECTED");}
  let location=response.headers().get("location").and_then(|v|v.to_str().ok()).ok_or("DSH_PROTOCOL")?;
  if base.join(location).map_err(|_|"DSH_PROTOCOL")?!=base{return Err("DSH_PROTOCOL");}
  let cookies:Vec<_>=response.headers().get_all("set-cookie").iter().filter_map(|v|v.to_str().ok()).filter_map(|s|s.split(';').next()).filter(|s|s.starts_with("dsh-auth-")).collect();
  if cookies.len()!=1 || cookies[0].len()>8192{return Err("DSH_PROTOCOL");}
  use sha2::{Digest,Sha256};
  let cookie: String = cookies[0].into();
  let scope=format!("{:x}",Sha256::digest(format!("{}\0{}",base,cookie).as_bytes()));
  Ok(Self{base,cookie,http,scope})
 }
 pub fn from_cookie(endpoint: &str, cookie: &str) -> Result<Self, &'static str> {
  let base=reqwest::Url::parse(endpoint).map_err(|_|"DSH_ADDRESS_INVALID")?;
  if !trusted_endpoint(&base) { return Err("DSH_HTTPS_REQUIRED"); }
  if !cookie.starts_with("dsh-auth-") || cookie.len()>8192 || cookie.contains(';') || cookie.chars().any(char::is_control) { return Err("DSH_PROTOCOL"); }
  let http=shared_http_client()?;
  use sha2::{Digest,Sha256};
  let scope=format!("{:x}",Sha256::digest(format!("{}\0{}",base,cookie).as_bytes()));
  Ok(Self{base,cookie:cookie.into(),http,scope})
 }
 pub fn cookie(&self) -> &str { &self.cookie }
 pub fn check_auth(&self) -> Result<(), &'static str> {
  let _ = self.rpc("session/list", json!({"_request":{}}))?;
  Ok(())
 }
 pub fn connect_or_recover(
  endpoint: &str,
  configured_token: Option<&str>,
  cache_dir: &std::path::Path,
  log_source: Option<&std::path::Path>,
 ) -> Result<Self, &'static str> {
  if let Some(saved_cookie) = load_cached_cookie(cache_dir, endpoint) {
   if let Ok(client) = Self::from_cookie(endpoint, &saved_cookie) {
    if cached_cookie_recently_verified(cache_dir, endpoint, 300_000) {
     return Ok(client);
    }
    match client.check_auth() {
     Ok(()) => {
      let _ = save_cached_cookie_atomic(cache_dir, endpoint, &saved_cookie);
      return Ok(client);
     }
     Err(e @ ("DSH_HISTORY_TIMEOUT" | "DSH_HISTORY_UNAVAILABLE" | "DSH_UNAVAILABLE")) => {
      // Network unreachable: keep cached session credential untouched and do not re-exchange token.
      return Err(e);
     }
     Err(_) => {}
    }
   }
  }
  let mut last_err = "DSH_AUTH_REJECTED";
  if let Some(tok) = configured_token.map(str::trim).filter(|s| !s.is_empty()) {
   match Self::connect(endpoint, tok) {
    Ok(client) => {
     let _ = save_cached_cookie_atomic(cache_dir, endpoint, &client.cookie);
     return Ok(client);
    }
    Err(e @ ("DSH_UNAVAILABLE" | "DSH_HISTORY_TIMEOUT" | "DSH_HISTORY_UNAVAILABLE")) => return Err(e),
    Err(e) => last_err = e,
   }
  }
  if let Some(log_path) = log_source {
   let fresh_token = extract_startup_token_from_log_tail(log_path, 64 * 1024)?;
   let client = Self::connect(endpoint, &fresh_token)?;
   let _ = save_cached_cookie_atomic(cache_dir, endpoint, &client.cookie);
   return Ok(client);
  }
  Err(last_err)
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
   let workspace=s["projections"]["values"].get("workingDirectory").filter(|v|!v.is_null()).or_else(||s.get("cwd").filter(|v|!v.is_null())).cloned().unwrap_or(Value::Null);
   output.push(json!({"id":id,"title":s["projections"]["values"]["title"].as_str().unwrap_or(id),"updated_at":s["updatedAt"],"running":running,"workspace":workspace,"source_kind":"dsh_native"}));
  }
  output.sort_by_key(|s|std::cmp::Reverse(s["updated_at"].as_i64().unwrap_or(0)));
  Ok(json!(output))
 }
 pub fn history(&self,address:SessionAddress)->Result<History,&'static str>{
  let h=project_history(address.clone(),&self.snapshot(&address,200)?)?;
  preview_references().lock().map_err(|_|"DSH_UNAVAILABLE")?.remember(&self.scope,&h);
  Ok(h)
 }
 pub fn live(&self,address:SessionAddress)->Result<Value,&'static str>{
  crate::dsh_live::view(self.clone(),self.scope.clone(),address)
 }
 pub(crate) fn follow_socket(&self,session:&str)->Result<(tungstenite::WebSocket<MaybeTlsStream<TcpStream>>,String),&'static str>{
  let mut url=self.base.join("api/remote.mux").map_err(|_|"DSH_PROTOCOL")?;url.set_scheme(if self.base.scheme()=="http" {"ws"} else {"wss"}).map_err(|_|"DSH_PROTOCOL")?;
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
  match ws.get_mut(){MaybeTlsStream::Rustls(s)=>{s.sock.set_read_timeout(Some(Duration::from_secs(1))).map_err(|_|"DSH_UNAVAILABLE")?;s.sock.set_write_timeout(Some(Duration::from_secs(1))).map_err(|_|"DSH_UNAVAILABLE")?;},MaybeTlsStream::Plain(s) if self.base.scheme()=="http" && trusted_endpoint(&self.base)=>{s.set_read_timeout(Some(Duration::from_secs(1))).map_err(|_|"DSH_UNAVAILABLE")?;s.set_write_timeout(Some(Duration::from_secs(1))).map_err(|_|"DSH_UNAVAILABLE")?;},_=>return Err("DSH_TLS_REQUIRED")}
  let stream=format!("bob-live-{}",crate::now_millis());
  ws.send(Message::Text(json!({"type":"open","streamId":stream,"endpoint":"session/follow","payload":{"args":{"request":{"address":{"kind":"session","sessionId":session},"maxMessages":1,"assistantStream":true}}}}).to_string().into())).map_err(|_|"DSH_UNAVAILABLE")?;
  Ok((ws,stream))
 }
 fn snapshot(&self,address:&SessionAddress,max_messages:usize)->Result<Value,&'static str>{
  let mut url=self.base.join("api/remote.mux").map_err(|_|"DSH_PROTOCOL")?;url.set_scheme(if self.base.scheme()=="http" {"ws"} else {"wss"}).map_err(|_|"DSH_PROTOCOL")?;
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
  match ws.get_mut(){MaybeTlsStream::Rustls(s)=>{s.sock.set_read_timeout(Some(Duration::from_secs(15))).map_err(|_|"DSH_UNAVAILABLE")?;s.sock.set_write_timeout(Some(Duration::from_secs(15))).map_err(|_|"DSH_UNAVAILABLE")?;},MaybeTlsStream::Plain(s) if self.base.scheme()=="http" && trusted_endpoint(&self.base)=>{s.set_read_timeout(Some(Duration::from_secs(15))).map_err(|_|"DSH_UNAVAILABLE")?;s.set_write_timeout(Some(Duration::from_secs(15))).map_err(|_|"DSH_UNAVAILABLE")?;},_=>return Err("DSH_TLS_REQUIRED")}
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
  // For older v3 sessions where `session/projections` returns `kind == "migration-required"`,
  // fall back to the cached projection cut (`asOfSeq` + `cwd`) from `session/list`.
  let raw_projection=self.rpc("session/projections",json!({"request":{"sessionId":address.session_id}}))?;
  let (projection,header_cwd)=if raw_projection["kind"]=="sequenced" {
   (raw_projection,Value::Null)
  } else {
   let list_val=self.rpc("session/list",json!({"_request":{}}))?;
   let item=list_val["items"].as_array().and_then(|arr|arr.iter().find(|s|s["sessionId"].as_str()==Some(address.session_id.as_str()))).ok_or("DSH_PROTOCOL")?;
   let item_proj=&item["projections"];
   if item_proj["kind"]!="sequenced" && item_proj["kind"]!="cached" {return Err("DSH_PROTOCOL");}
   (item_proj.clone(),item.get("cwd").cloned().unwrap_or(Value::Null))
  };
  let current=projection["asOfSeq"].as_i64().filter(|n|*n>= -1).ok_or("DSH_PROTOCOL")?;
  if after==Some(current){return Ok(json!({"address":address,"cursor":current,"not_modified":true,"messages":[],"before":0,"source_requests":1,"source_bytes":serde_json::to_vec(&projection).map_err(|_|"DSH_PROTOCOL")?.len()}));}
  let started=Instant::now();
  let cursor=through.unwrap_or(current);
  if cursor>current {return Err("DSH_PAGE_CURSOR_INVALID");}
  let page=self.rpc("session/page",json!({"request":{"address":{"kind":"session","sessionId":address.session_id},"throughSeq":cursor,"beforeSeq":before.unwrap_or(cursor.checked_add(1).ok_or("DSH_PAGE_CURSOR_INVALID")?),"maxMessages":64}}))?;
  let mut snapshot=json!({"type":"snapshot","header":{"id":address.session_id,"cwd":header_cwd},"projections":projection,"cursor":cursor,"records":page["records"],"hasMore":page["hasMore"]});
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
  let document=["pdf","docx","xlsx","pptx","doc","xls","ppt","odt","ods","odp"].contains(&ext.as_str());
  let version=if document {json!(format!("document-v1:{}",stat["version"]))} else {stat["version"].clone()};
  if known.is_some_and(|k| !stat["version"].is_null() && k["version"]==version && k["bytes"]==stat["bytes"]) {
   return Ok(json!({"not_modified":true,"path":path,"connection_id":address.connection_id,"session_id":address.session_id,"version":version,"bytes":size}));
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
  let mut result=json!({"path":path,"extension":ext,"data":base64::engine::general_purpose::STANDARD.encode(&bytes),"bytes":size,"version":version,"sha256":format!("{:x}",Sha256::digest(&bytes)),"source":"current_workspace","connection_id":address.connection_id,"session_id":address.session_id});
  if document {result["document"]=crate::agy_directory::render_document_bytes(&bytes,&ext)?;}
  Ok(result)
 }
 pub fn resolve_session_workspace(&self,session:&str)->Option<String>{
  if let Ok(guard)=preview_references().lock() {
   if let Some(root)=guard.session_root(&self.scope,session) { return Some(root); }
  }
  if let Ok(list)=self.list() {
   if let Some(arr)=list.as_array() {
    for item in arr {
     if item["id"].as_str()==Some(session) {
      if let Some(w)=item["workspace"].as_str().filter(|s|!s.trim().is_empty()) {
       return Some(w.to_owned());
      }
     }
    }
   }
  }
  None
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
  if !["png","jpg","jpeg","webp","md","txt","json","yaml","yml","gd","tscn","js","css","html","pdf","docx","xlsx","pptx","doc","xls","ppt","odt","ods","odp"].contains(&ext.as_str()){return Err("DSH_FILE_TYPE_UNSUPPORTED");}
  Ok(ext)
}

#[cfg(test)] mod preview_tests {
 use super::*;
 #[test] fn allowed_preview_paths(){assert_eq!(preview_extension("snapshots/view.png").unwrap(),"png");assert_eq!(preview_extension("docs/progress.md").unwrap(),"md");}
 #[test] fn document_preview_paths(){for ext in ["pdf","docx","xlsx","pptx","doc","xls","ppt","odt","ods","odp"] {assert_eq!(preview_extension(&format!("docs/report.{ext}")).unwrap(),ext);}}
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
  History{address:SessionAddress{connection_id:"connection".into(),session_id:session.into()},workspace:Some(root.into()),messages:vec![crate::dsh_session::Message{sequence:1,role:"assistant".into(),text:text.into(),created_at:None}],partial:true,omitted:0,request_results:vec![]}
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

fn cookie_cache_path(cache_dir: &std::path::Path, endpoint: &str) -> std::path::PathBuf {
 use sha2::{Digest, Sha256};
 let key = format!("{:x}", Sha256::digest(endpoint.trim().as_bytes()));
 cache_dir.join("dsh-auth-sessions").join(format!("{key}.json"))
}

pub fn load_cached_cookie(cache_dir: &std::path::Path, endpoint: &str) -> Option<String> {
 let path = cookie_cache_path(cache_dir, endpoint);
 let bytes = std::fs::read(path).ok()?;
 let val: Value = serde_json::from_slice(&bytes).ok()?;
 if val["endpoint"].as_str()? != endpoint.trim() { return None; }
 let cookie = val["cookie"].as_str()?.trim();
 if cookie.starts_with("dsh-auth-") && cookie.len() <= 8192 { Some(cookie.to_owned()) } else { None }
}

fn cached_cookie_recently_verified(cache_dir: &std::path::Path, endpoint: &str, max_age_ms: i64) -> bool {
 let path = cookie_cache_path(cache_dir, endpoint);
 let Ok(bytes) = std::fs::read(path) else { return false; };
 let Ok(val) = serde_json::from_slice::<Value>(&bytes) else { return false; };
 let Some(updated_at) = val["updated_at"].as_i64() else { return false; };
 let age = crate::now_millis().saturating_sub(updated_at);
 (0..=max_age_ms).contains(&age)
}

pub fn save_cached_cookie_atomic(cache_dir: &std::path::Path, endpoint: &str, cookie: &str) -> Result<(), &'static str> {
 let path = cookie_cache_path(cache_dir, endpoint);
 if let Some(parent) = path.parent() {
  std::fs::create_dir_all(parent).map_err(|_| "DSH_CREDENTIAL_WRITE_FAILED")?;
 }
 let tmp = path.with_extension(format!("tmp-{}", crate::now_millis()));
 let payload = json!({
  "endpoint": endpoint.trim(),
  "cookie": cookie.trim(),
  "updated_at": crate::now_millis(),
 });
 std::fs::write(&tmp, payload.to_string().as_bytes()).map_err(|_| "DSH_CREDENTIAL_WRITE_FAILED")?;
 std::fs::rename(&tmp, &path).map_err(|_| "DSH_CREDENTIAL_WRITE_FAILED")?;
 Ok(())
}

/// Deterministically extract the latest startup token from a bounded tail of a controlled DSH log.
/// Never scans unbounded files or guesses arbitrary long strings without an explicit token marker.
pub fn extract_startup_token_from_log_tail(log_path: &std::path::Path, max_bytes: u64) -> Result<String, &'static str> {
 use std::io::{Seek, SeekFrom};
 let mut file = std::fs::File::open(log_path).map_err(|_| "DSH_LOG_UNAVAILABLE")?;
 let len = file.metadata().map_err(|_| "DSH_LOG_UNAVAILABLE")?.len();
 let limit = max_bytes.clamp(1024, 256 * 1024);
 let start = len.saturating_sub(limit);
 file.seek(SeekFrom::Start(start)).map_err(|_| "DSH_LOG_UNAVAILABLE")?;
 let mut buf = Vec::with_capacity((len - start) as usize);
 file.take(limit).read_to_end(&mut buf).map_err(|_| "DSH_LOG_UNAVAILABLE")?;
 let tail = String::from_utf8_lossy(&buf);
 for line in tail.lines().rev() {
  if let Some(tok) = parse_token_from_log_line(line) {
   return Ok(tok);
  }
 }
 Err("DSH_STARTUP_TOKEN_NOT_FOUND")
}

fn parse_token_from_log_line(line: &str) -> Option<String> {
 for marker in ["?token=", "&token=", "startup_token=", "startup token:"] {
  if let Some(idx) = line.find(marker) {
   let rest = line[idx + marker.len()..].trim_start();
   let candidate: String = rest
    .chars()
    .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
    .collect();
   if (16..=256).contains(&candidate.len()) {
    return Some(candidate);
   }
  }
 }
 None
}

#[cfg(test)]
mod auth_recovery_tests {
 use super::*;
 #[test]
 fn extracts_latest_startup_token_from_bounded_tail_and_ignores_random_strings() {
  let dir = tempfile::tempdir().unwrap();
  let log = dir.path().join("dsh.log");
  let noise = "a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6q7r8s9t0
";
  std::fs::write(&log, format!("{noise}Listening on https://127.0.0.1:4433/?token=first_token_1234567890
{noise}Restarted: https://127.0.0.1:4433/?token=second_token_abcdef123456
{noise}")).unwrap();
  let tok = extract_startup_token_from_log_tail(&log, 4096).unwrap();
  assert_eq!(tok, "second_token_abcdef123456");

  let log_no_marker = dir.path().join("noise.log");
  std::fs::write(&log_no_marker, noise.repeat(20)).unwrap();
  assert_eq!(extract_startup_token_from_log_tail(&log_no_marker, 4096).unwrap_err(), "DSH_STARTUP_TOKEN_NOT_FOUND");
 }

 #[test]
 fn atomic_cookie_cache_isolated_by_endpoint() {
  let dir = tempfile::tempdir().unwrap();
  save_cached_cookie_atomic(dir.path(), "https://node1.example.com/", "dsh-auth-1=cookie_value_one").unwrap();
  assert_eq!(
   load_cached_cookie(dir.path(), "https://node1.example.com/").as_deref(),
   Some("dsh-auth-1=cookie_value_one")
  );
  assert_eq!(load_cached_cookie(dir.path(), "https://node2.example.com/"), None);
 }
}

#[cfg(test)] mod node_endpoint_policy_tests {
 use super::*;
 #[test] fn loopback_http_only_remote_tls_and_no_embedded_secrets() {
  for raw in ["http://127.0.0.1:3080", "http://[::1]:3080", "https://node.example"] {assert!(trusted_endpoint(&reqwest::Url::parse(raw).unwrap()));}
  for raw in ["http://localhost:3080", "http://node.example", "http://192.168.1.2", "https://u:p@node.example", "https://node.example/?token=a", "https://node.example/path"] {assert!(!trusted_endpoint(&reqwest::Url::parse(raw).unwrap()));}
 }
}

fn sanitize_attachment_name(raw: &str) -> String {
 let base = raw.rsplit(['/', '\\']).next().unwrap_or("attachment.bin");
 let cleaned: String = base
  .chars()
  .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' })
  .collect();
 let trimmed = cleaned.trim_matches('.');
 if trimmed.is_empty() { "attachment.bin".to_owned() } else { trimmed.to_owned() }
}

fn find_remote_ssh_target_for_connection(data_dir: &std::path::Path, connection: &str) -> Option<(String, Option<String>, Option<String>)> {
 let grants = crate::claude_harness::load_grants(data_dir).ok()?;
 let mut ssh_for_conn = None;
 for g in grants {
  if g.connection_id == connection {
   if let Some(ref ssh) = g.remote_ssh_target {
    if !ssh.trim().is_empty() {
     ssh_for_conn = Some((ssh.clone(), g.remote_cwd.clone(), Some(g.cwd.clone())));
     break;
    }
   }
  }
 }
 ssh_for_conn
}

pub fn stage_attachments_for_prompt(
 data_dir: &std::path::Path,
 connection: &str,
 session: &str,
 workspace_hint: Option<&str>,
 text: &str,
 attachments: Option<&Value>,
) -> String {
 let Some(arr) = attachments.and_then(Value::as_array).filter(|a| !a.is_empty()) else {
  return text.to_owned();
 };
 use base64::Engine;
 let grant_opt = crate::claude_harness::find_grant(data_dir, connection, session).ok().flatten();
 let conn_ssh = find_remote_ssh_target_for_connection(data_dir, connection);
 let ssh_target = grant_opt
  .as_ref()
  .and_then(|g| g.remote_ssh_target.clone())
  .or_else(|| conn_ssh.as_ref().map(|(s, _, _)| s.clone()));
 let workspace = workspace_hint
  .filter(|s| !s.trim().is_empty())
  .map(str::to_owned)
  .or_else(|| grant_opt.as_ref().and_then(|g| g.remote_cwd.clone().or_else(|| Some(g.cwd.clone()))))
  .unwrap_or_else(|| "/tmp/bob_attachments".to_owned());

 let mut updated = text.to_owned();
 for item in arr.iter().take(8) {
  let Some(raw_name) = item.get("name").and_then(Value::as_str) else { continue };
  let Some(b64) = item.get("base64").and_then(Value::as_str).filter(|s| !s.is_empty()) else { continue };
  let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64.trim()) else { continue };
  if bytes.is_empty() || bytes.len() > 10 * 1024 * 1024 { continue; }
  let safe_name = sanitize_attachment_name(raw_name);
  let target_dir = format!("{}/.bob_attachments", workspace.trim_end_matches(['/', '\\']));
  let target_file = format!("{target_dir}/{safe_name}");
  let mut staged_ok = false;

  let local_ws_path = std::path::Path::new(&workspace);
  if ssh_target.is_none() || local_ws_path.exists() {
   let dir_path = local_ws_path.join(".bob_attachments");
   if std::fs::create_dir_all(&dir_path).is_ok() {
    let file_path = dir_path.join(&safe_name);
    if std::fs::write(&file_path, &bytes).is_ok() {
     #[cfg(unix)]
     {
      use std::os::unix::fs::PermissionsExt;
      let _ = std::fs::set_permissions(&file_path, std::fs::Permissions::from_mode(0o644));
     }
     staged_ok = true;
    }
   }
  }
  if !staged_ok {
   if let Some(ref ssh) = ssh_target {
    let quoted_dir = target_dir.replace('\'', "'\\''");
    let quoted_file = target_file.replace('\'', "'\\''");
    let remote_cmd = format!("mkdir -p '{quoted_dir}' && cat > '{quoted_file}' && chmod 644 '{quoted_file}'");
    if let Ok(mut child) = std::process::Command::new("ssh")
     .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=6", ssh, &remote_cmd])
     .stdin(std::process::Stdio::piped())
     .stdout(std::process::Stdio::null())
     .stderr(std::process::Stdio::null())
     .spawn()
    {
     if let Some(mut stdin) = child.stdin.take() {
      let _ = stdin.write_all(&bytes);
     }
     if let Ok(status) = child.wait() {
      staged_ok = status.success();
     }
    }
   }
  }
  if !staged_ok {
   let fallback_dir = data_dir.join("bob_attachments");
   if std::fs::create_dir_all(&fallback_dir).is_ok() {
    let fallback_file = fallback_dir.join(&safe_name);
    if std::fs::write(&fallback_file, &bytes).is_ok() {
     #[cfg(unix)]
     {
      use std::os::unix::fs::PermissionsExt;
      let _ = std::fs::set_permissions(&fallback_file, std::fs::Permissions::from_mode(0o644));
     }
    }
   }
  }

  let size = item.get("size").and_then(Value::as_u64).unwrap_or(bytes.len() as u64);
  let old_marker_sized = format!("[Attachment: {raw_name} ({size} bytes)]");
  let old_marker_plain = format!("[Attachment: {raw_name}]");
  let new_marker = format!("[Attachment: {raw_name} -> {target_file}]");
  if updated.contains(&old_marker_sized) {
   updated = updated.replace(&old_marker_sized, &new_marker);
  } else if updated.contains(&old_marker_plain) {
   updated = updated.replace(&old_marker_plain, &new_marker);
  } else {
   updated = format!("{updated}\n\n{new_marker}");
  }
 }
 updated
}

#[cfg(test)]
mod attachment_stage_tests {
 use super::*;
 use base64::Engine;
 use serde_json::json;

 #[test]
 fn stages_attachment_bytes_into_session_workspace_and_rewrites_marker() {
  let tmp = std::env::temp_dir().join(format!("bob_att_test_{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&tmp);
  let ws_dir = tmp.join("ws");
  std::fs::create_dir_all(&ws_dir).unwrap();
  let ws_str = ws_dir.to_string_lossy().to_string();
  let png_b64 = base64::engine::general_purpose::STANDARD.encode(b"\x89PNG");
  let attachments = json!([
   {
    "name": "1.png",
    "size": 4,
    "mime": "image/png",
    "base64": png_b64
   }
  ]);
  let raw_prompt = "请参考这张图\n\n[Attachment: 1.png (4 bytes)]";
  let staged = stage_attachments_for_prompt(
   &tmp,
   "local",
   "sess-1",
   Some(&ws_str),
   raw_prompt,
   Some(&attachments),
  );
  let expected_path = format!("{}/.bob_attachments/1.png", ws_str.trim_end_matches('/'));
  assert!(staged.contains(&format!("[Attachment: 1.png -> {expected_path}]")));
  let written = std::fs::read(ws_dir.join(".bob_attachments").join("1.png")).unwrap();
  assert_eq!(written, b"\x89PNG");
  let _ = std::fs::remove_dir_all(&tmp);
 }
 fn _noop() {
 }
}
