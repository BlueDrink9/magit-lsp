//! Plain editor conformance self-tests and real-binary protocol smoke test.
#[path = "common/stdio.rs"] mod stdio;
#[path = "common/fake_editor.rs"] mod fake_editor;
#[path = "common/mod.rs"] mod fixtures;
use fake_editor::{FakeEditor,Profile};
use serde_json::{json,Value};
use std::time::Duration;
use std::fs;

struct ClientPump { server: stdio::TestServer, editor: FakeEditor, next_id: i64, replies: std::collections::HashMap<i64,Value>, cancel_on_edit: bool }
impl ClientPump {
 fn new(mut server: stdio::TestServer, profile: Profile) -> Self {
  let init=server.initialize(json!({"window":{"workDoneProgress":true},"general":{"positionEncodings":["utf-8","utf-16","utf-32"]}}));
  assert!(init["result"]["capabilities"].is_object());
  let mut editor=FakeEditor::new(profile);
  editor.encoding=match init["result"]["capabilities"]["positionEncoding"].as_str().unwrap_or("utf-16") {"utf-8"=>"utf-8","utf-16"=>"utf-16","utf-32"=>"utf-32",other=>panic!("invalid negotiated encoding: {other}")};
  Self{server,editor,next_id:2,replies:Default::default(),cancel_on_edit:false}
 }
 fn send(&mut self, mut message: Value) -> i64 { let id=self.next_id; if message.get("id").is_some(){self.next_id+=1; message["id"]=json!(id);} self.server.send(message); id }
 fn dispatch(&mut self, request:&Value) { for frame in self.editor.dispatch(request){self.server.send(frame);} }
 fn step(&mut self) {
  let message=self.server.recv();
  if message["method"].is_string() && !message["id"].is_null(){ self.dispatch(&message); return; }
  if let Some(id)=message["id"].as_i64(){assert!(self.replies.insert(id,message).is_none(),"duplicate response");return;}
  if self.cancel_on_edit && message["method"]=="view/effects" && message["params"]["effects"].as_array().is_some_and(|e|e.iter().any(|e|e["type"]=="editDocument")) {
   let token=self.server.notifications.iter().rev().find(|m|m["method"]=="$/progress"&&m["params"]["value"]["kind"]=="begin").expect("commit began")["params"]["token"].clone();
   self.send(json!({"jsonrpc":"2.0","method":"window/workDoneProgress/cancel","params":{"token":token}}));self.cancel_on_edit=false;
  }
  self.server.notifications.push(message);
 }
 fn until(&mut self, condition:impl Fn(&Self)->bool) { let deadline=std::time::Instant::now()+Duration::from_secs(15); while !condition(self){assert!(std::time::Instant::now()<deadline,"pump condition timed out");self.step();} }
 fn until_response(&mut self, id:i64)->Value {self.until(|c|c.replies.contains_key(&id));self.replies.remove(&id).unwrap()}
 fn finish(mut self) {let id=self.send(json!({"jsonrpc":"2.0","id":0,"method":"shutdown","params":null}));assert_eq!(self.until_response(id)["result"],Value::Null);self.send(json!({"jsonrpc":"2.0","method":"exit","params":null}));assert!(self.server.finish(Duration::from_secs(10)).success());}
}

#[test]
fn real_binary_lifecycle_and_current_unsupported_methods_are_honest(){
 let mut client=ClientPump::new(stdio::TestServer::spawn(&[]),Profile::Standard);
 let uri="file:///tmp/plain-harness.txt";
 let frame=client.editor.open(uri,"hello\n");client.send(frame);
 let frame=client.editor.edit(uri,"edited\n");client.send(frame);
 let frame=client.editor.save(uri);client.send(frame);
 assert_eq!(client.editor.text(uri),"edited\n");
 let frame=client.editor.code_action(uri,json!({"start":{"line":0,"character":0},"end":{"line":0,"character":1}}));let ca_id=client.send(frame);
 let frame=client.editor.execute_command("magit.command",json!([]));let ec_id=client.send(frame);
 // Await the second request first: the pump must retain the first response.
 let ec=client.until_response(ec_id);assert!(ec["error"].is_object());
 let ca=client.until_response(ca_id);assert!(ca.get("result").is_some()||ca["error"].is_object());
 let frame=client.editor.close(uri);client.send(frame);assert!(!client.editor.documents.contains_key(uri));
 client.finish();
}

#[test]
fn all_documented_profiles_have_assertable_behavior(){
 let profiles=[Profile::DecomposedChanges,Profile::DebouncedDidChange,Profile::ReorderEcho,Profile::NoDocumentChanges,Profile::NoShowDocument,Profile::CrlfNormalise,Profile::Utf16];
 for p in profiles {let mut e=FakeEditor::new(p);let uri="file:///x";e.open(uri,"a\nb\n");match p {
  Profile::DecomposedChanges=>{let n=e.edit(uri,"A\nB\n");let c=n["params"]["contentChanges"].as_array().unwrap();assert_eq!(c.len(),3);assert!(c.iter().all(|v|v["range"].is_object()));}
 Profile::DebouncedDidChange=>{e.edit(uri,"queued");assert!(e.has_pending());let sent=e.flush();assert!(!e.has_pending());assert_eq!(sent.len(),1);assert_eq!(sent[0]["params"]["contentChanges"][0]["text"],"queued");}
 Profile::ReorderEcho=>{let req=json!({"jsonrpc":"2.0","id":17,"method":"workspace/applyEdit","params":{"edit":{"changes":{"file:///x":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"z"}]}}}});let response=e.respond(&req);let echo=e.edit(uri,"echo");assert_eq!(response["id"],17);assert_eq!(response["result"]["applied"],true);assert_eq!(echo["params"]["textDocument"]["version"],3);assert_eq!(e.text(uri),"echo");assert_eq!(e.outbound[0]["method"],"workspace/applyEdit");}
  Profile::NoDocumentChanges=>{assert_eq!(e.apply_edit(&json!({"documentChanges":[{"textDocument":{"uri":uri,"version":1},"edits":[]}]}))["applied"],false);assert_eq!(e.apply_edit(&json!({"changes":{"file:///x":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"z"}]}}))["applied"],true);}
 Profile::NoShowDocument=>{assert_eq!(e.respond(&json!({"id":5,"method":"window/showDocument"}))["result"]["success"],false);}
 Profile::CrlfNormalise=>{e.documents.get_mut(uri).unwrap().text="a\r\nb".into();e.apply_edit(&json!({"changes":{"file:///x":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"z"}]}}));assert!(!e.text(uri).contains("\r\n"));}
 Profile::Utf16=>{e.documents.get_mut(uri).unwrap().text="😀x".into();assert_eq!(e.apply_edit(&json!({"changes":{"file:///x":[{"range":{"start":{"line":0,"character":2},"end":{"line":0,"character":3}},"newText":"y"}]}}))["applied"],true);assert_eq!(e.text(uri),"😀y");}
 Profile::Standard=>unreachable!() }}
}

#[test]
fn versioned_apply_edit_validates_before_mutating_and_supports_multiple_changes(){let mut e=FakeEditor::new(Profile::Standard);let u="file:///d";e.open(u,"a\nb\n");let result=e.apply_edit(&json!({"documentChanges":[{"textDocument":{"uri":u,"version":1},"edits":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"A"},{"range":{"start":{"line":1,"character":0},"end":{"line":1,"character":1}},"newText":"B"}]}]}));assert_eq!(result["applied"],true);assert_eq!(e.text(u),"A\nB\n");let before=e.text(u).to_owned();let stale=e.apply_edit(&json!({"documentChanges":[{"textDocument":{"uri":u,"version":1},"edits":[]}]}));assert_eq!(stale["applied"],false);assert!(stale["failureReason"].is_string());assert_eq!(e.text(u),before);}

#[test]
fn overlapping_and_malformed_edits_reject_atomically_across_documents(){let mut e=FakeEditor::new(Profile::Standard);e.open("file:///a","abcd");e.open("file:///b","wxyz");let result=e.apply_edit(&json!({"changes":{"file:///a":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":2}},"newText":"A"},{"range":{"start":{"line":0,"character":1},"end":{"line":0,"character":3}},"newText":"B"}],"file:///b":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"W"}]}}));assert_eq!(result["applied"],false);assert_eq!(e.text("file:///a"),"abcd");assert_eq!(e.text("file:///b"),"wxyz");}

#[test]
fn utf16_surrogate_midpoint_is_rejected_and_crlf_lines_are_addressable(){let mut e=FakeEditor::new(Profile::Utf16);e.open("file:///u","😀x\r\ny");assert_eq!(e.apply_edit(&json!({"changes":{"file:///u":[{"range":{"start":{"line":0,"character":1},"end":{"line":0,"character":2}},"newText":"z"}]}}))["applied"],false);assert_eq!(e.apply_edit(&json!({"changes":{"file:///u":[{"range":{"start":{"line":1,"character":0},"end":{"line":1,"character":1}},"newText":"Y"}]}}))["applied"],true);assert_eq!(e.text("file:///u"),"😀x\r\nY");}

#[test]
fn reconstructed_trace_is_valid_jsonl(){for line in fs::read_to_string("tests/fixtures/lsp_traces/reconstructed-plain.jsonl").unwrap().lines(){let _:Value=serde_json::from_str(line).unwrap();}}

fn replacement(start:u64,end:u64,text:&str)->Value {json!({"range":{"start":{"line":0,"character":start},"end":{"line":0,"character":end}},"newText":text})}

#[test]
fn original_snapshot_order_and_atomic_rejection_matrix(){
 let a="file:///a";let b="file:///b";
 let mut e=FakeEditor::new(Profile::Standard);e.open(a,"abcdef");e.open(b,"uvwxyz");
 // Deliberately reversed and length-changing: all offsets use the original.
 assert_eq!(e.apply_edit(&json!({"documentChanges":[{"textDocument":{"uri":a,"version":1},"edits":[replacement(4,6,"!"),replacement(0,1,"LONG")]},{"textDocument":{"uri":b,"version":1},"edits":[replacement(0,1,"U")]}]}))["applied"],true);
 assert_eq!(e.text(a),"LONGbcd!");assert_eq!(e.text(b),"Uvwxyz");
 let valid=json!({"textDocument":{"uri":a,"version":2},"edits":[replacement(0,1,"X")]});
 let invalids=vec![
  json!({"textDocument":{"uri":b,"version":1},"edits":[]}),
  json!({"textDocument":{"uri":b,"version":"2"},"edits":[]}),
  json!({"textDocument":{"uri":b},"edits":[]}),
  json!({"textDocument":{"uri":b,"version":2},"edits":{}}),
  json!({"textDocument":{"uri":b,"version":2},"edits":[replacement(0,99,"x")]}),
  json!({"textDocument":{"uri":b,"version":2},"edits":[replacement(3,1,"x")]}),
  json!({"textDocument":{"uri":b,"version":2},"edits":[replacement(0,3,"x"),replacement(2,4,"y")]}),
  json!({"textDocument":{"uri":a,"version":2},"edits":[]}),
  json!({"kind":"create","uri":"file:///new"}),
 ];
 for invalid in invalids {let result=e.apply_edit(&json!({"documentChanges":[valid.clone(),invalid]}));assert_eq!(result["applied"],false,"{result}");assert!(result["failureReason"].is_string());assert_eq!(e.text(a),"LONGbcd!");assert_eq!(e.text(b),"Uvwxyz");assert_eq!(e.documents[a].version,2);assert_eq!(e.documents[b].version,2);}
 for edit in [json!({"documentChanges":{}}),json!({"changes":{a:[replacement(0,1,"X")],b:{}}}),json!({"changes":{a:[replacement(0,1,"X")],"file:///missing":[]}}),json!({"documentChanges":[],"changes":{}})] {assert_eq!(e.apply_edit(&edit)["applied"],false);assert_eq!(e.text(a),"LONGbcd!");assert_eq!(e.documents[a].version,2);}
}

#[test]
fn utf8_utf16_utf32_bounds_use_original_crlf_snapshots(){
 for (encoding,start,end,invalid) in [("utf-8",4,5,Some(1)),("utf-16",2,3,Some(1)),("utf-32",1,2,None)] {
  let mut e=FakeEditor::new(Profile::Standard);e.encoding=encoding;e.open("file:///u","😀x\r\ny");
  if let Some(mid)=invalid {assert_eq!(e.apply_edit(&json!({"changes":{"file:///u":[replacement(mid,start,"bad")]}}))["applied"],false);assert_eq!(e.text("file:///u"),"😀x\r\ny");}
  assert_eq!(e.apply_edit(&json!({"changes":{"file:///u":[replacement(start,end,"Y")]}}))["applied"],true);assert_eq!(e.text("file:///u"),"😀Y\r\ny");
  assert_eq!(e.apply_edit(&json!({"changes":{"file:///u":[replacement(end+1,end+1,"bad")]}}))["applied"],false);
 }
}

#[test]
fn many_decomposed_ranges_replay_sequentially_in_one_notification(){
 for encoding in ["utf-8","utf-16","utf-32"] {
  let uri="file:///decomposed";let mut e=FakeEditor::new(Profile::DecomposedChanges);e.encoding=encoding;e.open(uri,"😀old\r\ntail");
  let expected=(0..25).map(|i|format!("😀 line {i}\n")).collect::<String>();let notification=e.edit(uri,&expected);
  let changes=notification["params"]["contentChanges"].as_array().unwrap();assert_eq!(changes.len(),26);assert_eq!(notification["params"]["textDocument"]["version"],2);
  let mut replay=FakeEditor::new(Profile::Standard);replay.encoding=encoding;replay.open(uri,"😀old\r\ntail");
  for change in changes {assert!(change["range"].is_object());assert_eq!(replay.apply_edit(&json!({"changes":{uri:[{"range":change["range"],"newText":change["text"]}]}}))["applied"],true);}
  assert_eq!(replay.text(uri),expected);assert_eq!(e.text(uri),expected);
 }
}

#[test]
fn debounce_releases_no_echo_until_flush_on_actual_transport(){
 let mut client=ClientPump::new(stdio::TestServer::spawn(&[]),Profile::DebouncedDidChange);
 let uri="file:///debounce";let open=client.editor.open(uri,"a");client.send(open);
 let start=client.server.sent.len();
 client.dispatch(&json!({"jsonrpc":"2.0","id":"script/debounce","method":"workspace/applyEdit","params":{"edit":{"changes":{uri:[replacement(0,1,"b")]}}}}));
 assert_eq!(client.server.sent.len(),start+1);assert!(client.editor.has_pending());assert_eq!(client.editor.text(uri),"b");
 for frame in client.editor.flush(){client.send(frame);}
 assert!(!client.editor.has_pending());assert_eq!(client.server.sent.len(),start+2);assert_eq!(client.server.sent[start]["id"],"script/debounce");assert_eq!(client.server.sent[start+1]["method"],"textDocument/didChange");assert_eq!(client.server.sent[start+1]["params"]["textDocument"]["version"],2);
 client.finish();
}

#[test]
fn shared_dispatch_captures_real_send_order_for_echo_profiles(){for (profile,expected) in [(Profile::Standard,["textDocument/didChange","response"]),(Profile::ReorderEcho,["response","textDocument/didChange"])] {
 let mut client=ClientPump::new(stdio::TestServer::spawn(&[]),profile);
 let open=client.editor.open("file:///order","a");client.send(open);
 // Scripted server request, but actual dispatcher and pipe writes, not production Plain acceptance.
 let request=json!({"jsonrpc":"2.0","id":"script/apply","method":"workspace/applyEdit","params":{"edit":{"documentChanges":[{"textDocument":{"uri":"file:///order","version":1},"edits":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"b"}]}]}}});
 let start=client.server.sent.len();client.dispatch(&request);
 let writes=&client.server.sent[start..];assert_eq!(writes.iter().map(|m|m["method"].as_str().unwrap_or("response")).collect::<Vec<_>>(),expected);assert!(writes.iter().any(|m|m["id"]=="script/apply"));assert_eq!(client.editor.text("file:///order"),"b");
 client.dispatch(&json!({"jsonrpc":"2.0","id":"script/show","method":"window/showDocument","params":{"uri":"file:///order"}}));assert_eq!(client.server.sent.last().unwrap()["result"]["success"],true);
 client.dispatch(&json!({"jsonrpc":"2.0","id":"script/unknown","method":"unknown"}));assert_eq!(client.server.sent.last().unwrap()["error"]["code"],-32601);
 client.finish();
}}

#[test]
fn live_binary_progress_requests_are_serviced_while_command_runs(){
 let (repo,_)=fixtures::RepoFixture::new().commit("init",[("f.txt",fixtures::TEN_LINES)]).modify_standard().build();
 let head=repo.git(&["rev-parse","HEAD"]);
 let mut client=ClientPump::new(stdio::TestServer::spawn(&[]),Profile::Standard);
 let id=client.send(json!({"jsonrpc":"2.0","id":0,"method":"view/open","params":{"kind":"status","args":{"repo_path":repo.path().to_string_lossy()},"viewport":{"cols":80,"rows":24}}}));let opened=client.until_response(id);let view=opened["result"]["id"].as_str().expect("opened view").to_owned();
 let id=client.send(json!({"jsonrpc":"2.0","id":0,"method":"view/invoke","params":{"viewId":view,"command":"status.stage-all"}}));let response=client.until_response(id);assert!(response.get("result").is_some(),"{response}");
 assert!(repo.git(&["diff","--cached","--name-only"]).contains("f.txt"));
 // Cancel only when the actual commit editor is waiting. Cancelling on begin
 // races the setup rev-parse, whose killed process returns an RPC error.
 client.cancel_on_edit=true;
 let id=client.send(json!({"jsonrpc":"2.0","id":0,"method":"view/invoke","params":{"viewId":view,"command":"commit.create"}}));assert!(client.until_response(id).get("result").is_some());
 client.until(|c| !c.cancel_on_edit && c.server.notifications.iter().filter(|m|m["method"]=="$/progress"&&m["params"]["value"]["kind"]=="end").count()==2);
 let requests:Vec<_>=client.editor.outbound.iter().filter(|m|m["method"]=="window/workDoneProgress/create").collect();assert_eq!(requests.len(),2);
 assert_ne!(requests[0]["id"],requests[1]["id"]);
 for request in requests {assert!(client.server.sent.iter().any(|m|m["id"]==request["id"]&&m.get("result")==Some(&Value::Null)));let token=&request["params"]["token"];let phases:Vec<_>=client.server.notifications.iter().filter(|m|m["method"]=="$/progress"&&m["params"]["token"]==*token).map(|m|m["params"]["value"]["kind"].as_str().unwrap()).collect();assert_eq!(phases.first(),Some(&"begin"));assert_eq!(phases.last(),Some(&"end"));assert!(phases[1..phases.len()-1].iter().all(|p|*p=="report"));}
 assert!(client.server.sent.iter().any(|m|m["method"]=="window/workDoneProgress/cancel"));assert_eq!(repo.git(&["rev-parse","HEAD"]),head);
 client.finish();
}
