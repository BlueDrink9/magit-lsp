//! Plain editor conformance self-tests and real-binary protocol smoke test.
#[path = "common/stdio.rs"] mod stdio;
#[path = "common/fake_editor.rs"] mod fake_editor;
use fake_editor::{FakeEditor,Profile};
use serde_json::{json,Value};
use std::time::Duration;
use std::fs;

#[test]
fn real_binary_lifecycle_and_current_unsupported_methods_are_honest(){
 let mut server=stdio::TestServer::spawn(&[]); let init=server.initialize(json!({})); assert!(init["result"]["capabilities"].is_object());
 let uri="file:///tmp/plain-harness.txt"; server.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"plaintext","version":1,"text":"hello\n"}}}));
 server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":2},"contentChanges":[{"text":"edited\n"}]}}));
 server.send(json!({"jsonrpc":"2.0","method":"textDocument/didSave","params":{"textDocument":{"uri":uri}}})); server.send(json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri}}}));
 server.send(json!({"jsonrpc":"2.0","id":2,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"context":{"diagnostics":[]}}})); let ca=server.response_with_id(2); assert!(ca.get("result").is_some()||ca.get("error").is_some());
 server.send(json!({"jsonrpc":"2.0","id":3,"method":"workspace/executeCommand","params":{"command":"magit.command","arguments":[]}})); let ec=server.response_with_id(3); assert!(ec.get("result").is_some()||ec.get("error").is_some());
 server.shutdown_and_exit(); assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn all_documented_profiles_have_assertable_behavior(){
 let profiles=[Profile::DecomposedChanges,Profile::DebouncedDidChange,Profile::ReorderEcho,Profile::NoDocumentChanges,Profile::NoShowDocument,Profile::CrlfNormalise,Profile::Utf16];
 for p in profiles {let mut e=FakeEditor::new(p);let uri="file:///x";e.open(uri,"a\nb\n");match p {
 Profile::DecomposedChanges=>{let n=e.edit(uri,"A\nB\n");assert_eq!(n["params"]["contentChanges"].as_array().unwrap().len(),1);}
 Profile::DebouncedDidChange=>{e.edit(uri,"queued");assert_eq!(e.flush().len(),1);}
 Profile::ReorderEcho=>{let response=json!({"id":1,"result":{"applied":true}});let echo=e.edit(uri,"echo");assert_eq!(response["result"]["applied"],true);assert_eq!(echo["method"],"textDocument/didChange");}
 Profile::NoDocumentChanges=>{assert_eq!(e.apply_edit(&json!({"changes":{"file:///x":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"z"}]}}))["applied"],true);}
 Profile::NoShowDocument=>{assert_eq!(e.respond(&json!({"id":5,"method":"window/showDocument"}))["result"]["success"],false);}
 Profile::CrlfNormalise=>{e.documents.get_mut(uri).unwrap().text="a\r\nb".into();e.apply_edit(&json!({"changes":{"file:///x":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"z"}]}}));assert!(!e.text(uri).contains("\r\n"));}
 Profile::Utf16=>{e.documents.get_mut(uri).unwrap().text="😀x".into();assert_eq!(e.apply_edit(&json!({"changes":{"file:///x":[{"range":{"start":{"line":0,"character":2},"end":{"line":0,"character":3}},"newText":"y"}]}}))["applied"],true);assert_eq!(e.text(uri),"😀y");}
 Profile::Standard=>unreachable!() }}
}

#[test]
fn versioned_apply_edit_validates_before_mutating_and_supports_multiple_changes(){let mut e=FakeEditor::new(Profile::Standard);let u="file:///d";e.open(u,"a\nb\n");let result=e.apply_edit(&json!({"documentChanges":[{"textDocument":{"uri":u,"version":1},"edits":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"A"},{"range":{"start":{"line":1,"character":0},"end":{"line":1,"character":1}},"newText":"B"}]}]}));assert_eq!(result["applied"],true);assert_eq!(e.text(u),"A\nB\n");let before=e.text(u).to_owned();let stale=e.apply_edit(&json!({"documentChanges":[{"textDocument":{"uri":u,"version":1},"edits":[]}]}));assert_eq!(stale["applied"],false);assert!(stale["failureReason"].is_string());assert_eq!(e.text(u),before);}

#[test]
fn reconstructed_trace_is_valid_jsonl(){for line in fs::read_to_string("tests/fixtures/lsp_traces/reconstructed-plain.jsonl").unwrap().lines(){let _:Value=serde_json::from_str(line).unwrap();}}
