use magit_lsp::docstore::{Change, Encoding};
use magit_lsp::presentation::keys::{classify, derive_edit, inclusive_lines, key_name, text_hash, Outcome, State};

fn change(start:(u32,u32),end:(u32,u32),text:&str)->Change{Change{start:Some(start),end:Some(end),text:text.into()}}

#[test]
fn derives_ordered_encoded_edits_and_server_echo_hashes(){
    let edits=[change((0,1),(0,1),"x"),change((0,2),(0,2),"y")];
    let e=derive_edit("ab",1,2,&edits,Encoding::Utf8);
    assert_eq!(e.resulting_hash,text_hash("axyb"));
    let utf=derive_edit("😀\r\nz",4,5,&[change((0,2),(0,2),"x")],Encoding::Utf16);
    assert_eq!(utf.resulting_hash,text_hash("😀x\r\nz"));
    let crlf=derive_edit("a\r\nb",1,2,&[change((0,1),(0,1),"!")],Encoding::Utf8);
    assert_eq!(crlf.resulting_hash,text_hash("a!\r\nb"));
}

#[test]
fn classifies_supported_patterns_and_requires_related_pair(){
    let mut state=State::Ready;
    let ins=derive_edit("ab",1,2,&[change((0,1),(0,1),"x")],Encoding::Utf16);
    assert_eq!(classify(&mut state,ins,0,250),Outcome::Accept("x".into()));
    let del=derive_edit("abcd",1,2,&[change((0,1),(0,3),"")],Encoding::Utf16);
    assert_eq!(classify(&mut state,del,10,250),Outcome::Wait);
    let pair=derive_edit("ad",2,3,&[change((0,1),(0,1),"+")],Encoding::Utf16);
    assert_eq!(classify(&mut state,pair,20,250),Outcome::Accept("Plus".into()));
    let del=derive_edit("abc",3,4,&[change((0,1),(0,2),"")],Encoding::Utf16);
    assert_eq!(classify(&mut state,del,10,250),Outcome::Wait);
    assert!(matches!(classify(&mut state,derive_edit("zz",4,5,&[change((0,1),(0,1),"z")],Encoding::Utf16),20,250),Outcome::Reject{repair:true}));
    let del=derive_edit("abc",5,6,&[change((0,1),(0,2),"")],Encoding::Utf16);
    assert_eq!(classify(&mut state,del,10,250),Outcome::Wait);
    assert!(matches!(classify(&mut state,derive_edit("ac",6,7,&[change((0,1),(0,1),"z")],Encoding::Utf16),261,250),Outcome::Reject{repair:true}));
    // A ranged replacement is judged by its submitted payload, never by a
    // whole-document delta whose common prefix/suffix trims overlapping text.
    let overlap=derive_edit(" a",1,2,&[change((0,0),(0,1),"s ")],Encoding::Utf8);
    assert_eq!(overlap.inserted,"s ");
    let mut replaced=State::Ready;
    assert_eq!(classify(&mut replaced,overlap,0,250),Outcome::Reject{repair:true});
    let single=derive_edit(" a",2,3,&[change((0,0),(0,1),"s")],Encoding::Utf8);
    assert_eq!(single.inserted,"s");
    assert_eq!(classify(&mut replaced,single,0,250),Outcome::Accept("s".into()));
}

#[test]
fn rejects_non_keys_and_maps_lines(){
    for text in ["\n","\t","paste"] { let mut s=State::Ready; let e=derive_edit("a",1,2,&[change((0,1),(0,1),text)],Encoding::Utf16); assert!(matches!(classify(&mut s,e,0,250),Outcome::Reject{repair:true})); }
    let mut s=State::Ready;
    let empty=derive_edit("a",1,2,&[change((0,1),(0,1),"")],Encoding::Utf16);
    assert_eq!(classify(&mut s,empty,0,250),Outcome::Reject{repair:true});
    let mut header=derive_edit("a",1,2,&[change((0,0),(0,0),"x")],Encoding::Utf16);
    header.header=true;
    assert_eq!(classify(&mut s,header,0,250),Outcome::Reject{repair:true});
    let newline=derive_edit("a\nb",1,2,&[change((0,1),(1,0),"")],Encoding::Utf16);
    assert_eq!(classify(&mut s,newline,0,250),Outcome::Wait);
    assert_eq!(key_name(' '),Some("Space".into())); assert_eq!(key_name('+'),Some("Plus".into()));
    assert_eq!(inclusive_lines(2,4,0),(2,3));
}

#[test]
fn paired_deletion_expires_at_timeout_boundary(){
    let mut state=State::Ready;
    let deletion=derive_edit("ab",1,2,&[change((0,0),(0,1),"")],Encoding::Utf16);
    assert_eq!(classify(&mut state,deletion,0,250),Outcome::Wait);
    let insertion=derive_edit("b",2,3,&[change((0,0),(0,0),"x")],Encoding::Utf16);
    assert_eq!(classify(&mut state,insertion,250,250),Outcome::Reject{repair:true});
}

#[test]
fn t034_profiles_have_direct_outcomes(){
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lsp_traces/plain_keys");
    for name in ["selection-replace-pair","selection-replace-single","batched-changes","undo-after-repair","rangeless-full-text","paste-multichar"] {
        let fixture=std::fs::read_to_string(root.join(format!("{name}.jsonl"))).unwrap();
        let mut state=State::Ready; let mut text=if name=="batched-changes" {String::new()} else {"abcd".to_owned()};
        let mut outcomes=Vec::new(); let mut version=1;
        for line in fixture.lines() {
            let json:serde_json::Value=serde_json::from_str(line).unwrap();
            let params=&json["params"]; let after=params["textDocument"]["version"].as_i64().unwrap() as i32;
            let mut changes=Vec::new();
            for item in params["contentChanges"].as_array().unwrap() {
                let text_insert=item["text"].as_str().unwrap().to_owned();
                let range=&item["range"];
                let (start,end)=if range.is_object(){let s=&range["start"];let e=&range["end"];(Some((s["line"].as_u64().unwrap() as u32,s["character"].as_u64().unwrap() as u32)),Some((e["line"].as_u64().unwrap() as u32,e["character"].as_u64().unwrap() as u32)))}else{(None,None)};
                changes.push(Change{start,end,text:text_insert});
            }
            let edit=derive_edit(&text,version,after,&changes,Encoding::Utf16);
            outcomes.push(classify(&mut state,edit,version as u64,250));
            // Replay the fixture's changes independently to supply the next pre-snapshot.
            for c in &changes { if let (Some(s),Some(e))=(c.start,c.end) {let a=offset_for_test(&text,s);let b=offset_for_test(&text,e);text.replace_range(a..b,&c.text)} else {text.clone_from(&c.text)} }
            version=after;
        }
        let expected=match name {"selection-replace-single"=>vec![Outcome::Accept("x".into())],"selection-replace-pair"=>vec![Outcome::Wait,Outcome::Accept("x".into())],_=>vec![Outcome::Reject{repair:true};outcomes.len()]};
        assert_eq!(outcomes,expected,"{name}");
    }
}

fn offset_for_test(text:&str,pos:(u32,u32))->usize { text.split_inclusive('\n').take(pos.0 as usize).map(str::len).sum::<usize>()+pos.1 as usize }
