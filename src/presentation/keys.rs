//! Pure classification of Plain-presentation edits as key input.
use crate::docstore::{Change, Encoding};
use crate::{commands::{Command, CommandStatus}, keymap::Keymap};
use std::hash::{Hash, Hasher};

/// Net change made by one document notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetEdit {
    /// Hash of the text before the notification.
    pub pre_hash: u64,
    /// Hash of the text after the notification.
    pub resulting_hash: u64,
    /// Document version before this edit.
    pub version_before: i32,
    /// Document version after this edit.
    pub version_after: i32,
    /// Original half-open LSP range, if it is one ranged edit.
    pub pre_range: Option<((u32, u32), (u32, u32))>,
    /// Text inserted by the net edit.
    pub inserted: String,
    /// Whether the authoritative tree identifies the edited line as a header.
    pub header: bool,
}

/// State of a plain key edit classifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State { Ready, PendingDeletion { edit: NetEdit, since_ms: u64 }, Executing }
impl Default for State { fn default() -> Self { Self::Ready } }

/// URI-owned classifier state and repair latch for one Plain document lifetime.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DocumentState {
    /// Candidate/pairing state for this URI.
    pub classifier: State,
    /// Whether a busy edit was dropped and requires authoritative repair.
    pub repair_due: bool,
    /// Exact next document version and text hash of the outstanding repair.
    pub repair_echo: Option<(u64, i32, [u8; 32])>,
}

/// Classification decision for an edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome { Accept(String), Wait, Reject { repair: bool } }

/// Ingress decision for one accepted document notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision { Wait, Dispatch(String), Repair, DropBusy }

/// Preflight result for a Plain key against the exact displayed view state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyPreflight {
    /// The key only extends a pending chord or menu prefix.
    Pending,
    /// A safe key may run through the existing session key handler.
    Allowed { command: Option<String>, key: String, menu: bool },
    /// The key is not bound in the current view context.
    Unbound,
    /// The key was refused, with the user-facing one-line reason.
    Refused { id: Option<String>, message: String },
}

/// Captured document transition, preserving the exact accepted inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedInput {
    /// Document immediately before the notification.
    pub pre: crate::docstore::Document,
    /// Ordered LSP changes as accepted by the document store.
    pub changes: Vec<Change>,
    /// Accepted version after applying the notification.
    pub post_version: i32,
    /// Exact accepted document text after applying the notification.
    pub post_text: String,
}

/// Capture an accepted transition only when its ordered edits derive its exact post-state.
pub fn capture(pre: &crate::docstore::Document, changes: &[Change], post: &crate::docstore::Document, encoding: Encoding) -> Option<AcceptedInput> {
    let mut derived = pre.clone();
    derived.change(post.version, changes, encoding).ok()?;
    (derived.text == post.text && derived.version == post.version).then(|| AcceptedInput {
        pre: pre.clone(),
        changes: changes.to_vec(),
        post_version: post.version,
        post_text: post.text.clone(),
    })
}

/// Classify an accepted change only when its captured post-state is exact.
pub fn decide(state: &mut State, pre: &str, changes: &[Change], before: i32, after: i32, post: &str, encoding: Encoding, now_ms: u64, timeout_ms: u64) -> Decision {
    let edit = derive_edit(pre, before, after, changes, encoding);
    let mut derived = pre.to_owned();
    for change in changes {
        match (change.start, change.end) {
            (Some(start), Some(end)) => {
                let (Some(a), Some(b)) = (offset(&derived, start, encoding), offset(&derived, end, encoding)) else { return Decision::Repair };
                if a > b { return Decision::Repair; }
                derived.replace_range(a..b, &change.text);
            }
            (None, None) => derived.clone_from(&change.text),
            _ => return Decision::Repair,
        }
    }
    if derived != post || edit.version_after != after { return Decision::Repair; }
    if matches!(state, State::Executing) { return Decision::DropBusy; }
    match classify(state, edit, now_ms, timeout_ms) {
        Outcome::Accept(key) => Decision::Dispatch(key),
        Outcome::Wait => Decision::Wait,
        Outcome::Reject { .. } => Decision::Repair,
    }
}

/// Whether `command` may accept character input under the active profile keymap.
pub fn eligible(command: &Command, keymap: &Keymap) -> bool {
    matches!(command.status, CommandStatus::Implemented)
        && !command.destructive
        && keymap.command(command.id).is_some()
}

/// Stable hash of canonical document text.
pub fn text_hash(text: &str) -> u64 { let mut h=std::collections::hash_map::DefaultHasher::new(); text.hash(&mut h); h.finish() }

/// Derive the net edit produced by ordered LSP changes applied to `pre`.
pub fn derive_edit(pre:&str, before:i32, after:i32, changes:&[Change], encoding:Encoding)->NetEdit {
    let mut result=pre.to_owned(); let mut simple=None; let mut complex=false;
    for c in changes { match (c.start,c.end) {
        (Some(a),Some(b))=>{ if simple.is_none() && changes.len()==1 {simple=Some((a,b));} let x=offset(&result,a,encoding); let y=offset(&result,b,encoding); if let (Some(x),Some(y))=(x,y){if x<=y {result.replace_range(x..y,&c.text)} else {complex=true}} else {complex=true} }
        (None,None)=>{result.clone_from(&c.text); complex=true}, _=>complex=true,
    }}
    // A supported single ranged change is keyed by its submitted payload: a
    // whole-document delta can trim overlapping prefix/suffix text and turn a
    // multi-character replacement into an unintended single key.
    let (range,inserted)=if complex || simple.is_none(){(None,delta(pre,&result))} else {(simple,changes[0].text.clone())};
    NetEdit{pre_hash:text_hash(pre),resulting_hash:text_hash(&result),version_before:before,version_after:after,pre_range:range,inserted,header:false}
}

fn offset(s:&str,p:(u32,u32),enc:Encoding)->Option<usize>{
    let mut line_start=0; for _ in 0..p.0 {line_start=s[line_start..].find('\n').map(|i|line_start+i+1)?;}
    let tail=&s[line_start..]; let line=tail.split('\n').next()?; let line=line.strip_suffix('\r').unwrap_or(line);
    let mut units=0;let mut bytes=0; for c in line.chars(){if units==p.1{return Some(line_start+bytes)} let n=match enc{Encoding::Utf8=>c.len_utf8() as u32,Encoding::Utf16=>c.len_utf16() as u32,Encoding::Utf32=>1};if units+n>p.1{return None}units+=n;bytes+=c.len_utf8();} (units==p.1).then_some(line_start+bytes)
}
fn delta(a:&str,b:&str)->String{let aa:Vec<char>=a.chars().collect();let bb:Vec<char>=b.chars().collect();let mut p=0;while p<aa.len().min(bb.len())&&aa[p]==bb[p]{p+=1}let mut n=0;while n<aa.len()-p&&n<bb.len()-p&&aa[aa.len()-1-n]==bb[bb.len()-1-n]{n+=1}bb[p..bb.len()-n].iter().collect()}

/// Name a printable character using the Plain key convention.
pub fn key_name(c:char)->Option<String>{if c==' '{Some("Space".into())}else if c=='+'{Some("Plus".into())}else if c.is_control(){None}else{Some(c.to_string())}}
/// Convert a half-open LSP line range to inclusive line endpoints.
pub fn inclusive_lines(start:u32,end:u32,end_column:u32)->(u32,u32){(start,if end>start&&end_column==0{end-1}else{end})}

/// Classify a single net edit, pairing a deletion only with its immediate related insertion.
pub fn classify(state:&mut State,edit:NetEdit,now_ms:u64,timeout_ms:u64)->Outcome{
    if matches!(state,State::Executing){return Outcome::Reject{repair:true}}
    if edit.header { *state=State::Ready; return Outcome::Reject{repair:true} }
    if let State::PendingDeletion{edit:deleted,since_ms}=state.clone(){
        *state=State::Ready;
        if now_ms.saturating_sub(since_ms)>=timeout_ms || edit.version_before!=deleted.version_after || edit.pre_hash!=deleted.resulting_hash || edit.inserted.chars().count()!=1 || edit.pre_range.map(|r|r.0)!=deleted.pre_range.map(|r|r.0){return Outcome::Reject{repair:true}}
        return key_name(edit.inserted.chars().next().unwrap()).map(Outcome::Accept).unwrap_or(Outcome::Reject{repair:true})
    }
    let Some(((sl,sc),(el,ec)))=edit.pre_range else{return Outcome::Reject{repair:true}};
    if edit.inserted.is_empty(){if (sl,sc)==(el,ec){return Outcome::Reject{repair:true}} *state=State::PendingDeletion{edit,since_ms:now_ms};return Outcome::Wait}
    if edit.inserted.chars().count()!=1{return Outcome::Reject{repair:true}}
    let shape=(sl,sc)==(el,ec)||(sl,sc)<(el,ec);
    if !shape{return Outcome::Reject{repair:true}}
    key_name(edit.inserted.chars().next().unwrap()).map(Outcome::Accept).unwrap_or(Outcome::Reject{repair:true})
}
