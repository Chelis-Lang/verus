//! Private interactive session: focused phase and bounded observation for one compiler callback.
use super::*;
use std::cell::{Cell,RefCell};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
static FOCUS: AtomicBool = AtomicBool::new(false);
static REPORT_BYTES:AtomicU64=AtomicU64::new(0);
type Messages = Vec<(ArcDynMessage, MessageLevel)>;
const BUFFER_BUDGET:usize=128*1024*1024;
struct Bounded { bytes: Vec<u8> }
impl Write for Bounded {
    fn write(&mut self,bytes:&[u8])->std::io::Result<usize> {
        if self.bytes.len().saturating_add(bytes.len()) > BUFFER_BUDGET {return Err(std::io::Error::other("focused report budget"));}
        let needed=self.bytes.len()+bytes.len();
        if needed>self.bytes.capacity(){let capacity=needed.max(self.bytes.capacity().saturating_mul(2)).clamp(4096,BUFFER_BUDGET);self.bytes.try_reserve_exact(capacity-self.bytes.len()).map_err(std::io::Error::other)?;}
        if self.bytes.capacity()>BUFFER_BUDGET{return Err(std::io::Error::other("focused report allocation budget"));}
        self.bytes.extend_from_slice(bytes);Ok(bytes.len())
    }
    fn flush(&mut self)->std::io::Result<()>{Ok(())}
}
fn active()->bool{std::env::var("SAIL_ENGINE").as_deref()==Ok("interactive")}
// Observation is independently bounded, never part of proof acceptance.
const OBSERVER_BYTES:usize=1024*1024;
const OBSERVER_NAMES:usize=4096;
const OBSERVER_OWNERS:usize=2*1024*1024;
static OBSERVER_OWNED:AtomicU64=AtomicU64::new(0);
static EMITTER:std::sync::Mutex<()>=std::sync::Mutex::new(());
struct ObserverOwner {bytes:u64}
impl ObserverOwner {
    fn acquire()->Option<Self>{const BYTES:u64=24*1024;OBSERVER_OWNED.try_update(Ordering::Relaxed,Ordering::Relaxed,|old|old.checked_add(BYTES).filter(|next|*next<=OBSERVER_OWNERS as u64)).ok()?;Some(Self {bytes:BYTES})}
}
impl Drop for ObserverOwner {fn drop(&mut self){OBSERVER_OWNED.fetch_sub(self.bytes,Ordering::Relaxed);}}
pub(super) struct Progress {identity:Option<String>,_owner:ObserverOwner}
impl serde::Serialize for Progress {fn serialize<S:serde::Serializer>(&self,serializer:S)->Result<S::Ok,S::Error>{serializer.serialize_str(self.identity.as_deref().unwrap_or(""))}}
impl Drop for Progress {fn drop(&mut self){drop(self.identity.take()); /* owner is released only after physical identity drop */}}
pub(super) struct Fields<T>(pub T);
pub(super) struct Field<V,T>(pub &'static str,pub V,pub T);
pub(super) trait FieldList {
    fn count(&self)->usize;
    fn contains(&self,key:&str)->bool;
    fn unique(&self)->bool;
    fn write<M:serde::ser::SerializeMap>(&self,map:&mut M)->Result<(),M::Error>;
}
impl FieldList for () {fn count(&self)->usize{0}fn contains(&self,_:&str)->bool{false}fn unique(&self)->bool{true}fn write<M:serde::ser::SerializeMap>(&self,_:&mut M)->Result<(),M::Error>{Ok(())}}
impl<V:serde::Serialize,T:FieldList> FieldList for Field<V,T> {
    fn count(&self)->usize{1+self.2.count()}
    fn contains(&self,key:&str)->bool{self.0==key || self.2.contains(key)}
    fn unique(&self)->bool{!self.2.contains(self.0) && self.2.unique()}
    fn write<M:serde::ser::SerializeMap>(&self,map:&mut M)->Result<(),M::Error>{map.serialize_entry(self.0,&self.1)?;self.2.write(map)}
}
impl<T:FieldList> serde::Serialize for Fields<T>{fn serialize<S:serde::Serializer>(&self,serializer:S)->Result<S::Ok,S::Error>{if !self.0.unique(){return Err(serde::ser::Error::custom("duplicate observer key"));}let mut map=serializer.serialize_map(Some(self.0.count()))?;self.0.write(&mut map)?;serde::ser::SerializeMap::end(map)}}
#[derive(serde::Serialize)]
pub(super) struct Observation<'a,P>{pub version:u32,pub phase:&'a str,pub event:&'a str,pub payload:P}
pub(super) struct ObservedFriendly<'a>(pub &'a Fun);
impl serde::Serialize for ObservedFriendly<'_>{fn serialize<S:serde::Serializer>(&self,serializer:S)->Result<S::Ok,S::Error>{let name=vir::ast_util::sail_bounded_friendly_name(&self.0.path,OBSERVER_NAMES).ok_or_else(||<S::Error as serde::ser::Error>::custom("observation friendly-name budget"))?;serializer.serialize_str(&name)}}
pub(super) struct ObservedModule<'a>(pub &'a vir::ast::Path);
impl serde::Serialize for ObservedModule<'_>{fn serialize<S:serde::Serializer>(&self,serializer:S)->Result<S::Ok,S::Error>{let name=bounded_module_name(self.0,OBSERVER_NAMES).ok_or_else(||serde::ser::Error::custom("observation module-name budget"))?;serializer.serialize_str(&name)}}
pub(super) struct ObservationWriter {bytes:Vec<u8>}
impl Write for ObservationWriter {
    fn write(&mut self,bytes:&[u8])->std::io::Result<usize>{let next=self.bytes.len().checked_add(bytes.len()).ok_or_else(||std::io::Error::other("observation overflow"))?;if next>OBSERVER_BYTES{return Err(std::io::Error::other("observation limit"));}
        if next>self.bytes.capacity(){let capacity=next.max(self.bytes.capacity().saturating_mul(2)).clamp(64,OBSERVER_BYTES);self.bytes.try_reserve_exact(capacity-self.bytes.len())?;}
        if self.bytes.capacity()>OBSERVER_BYTES{return Err(std::io::Error::other("observation capacity"));}self.bytes.extend_from_slice(bytes);Ok(bytes.len())}
    fn flush(&mut self)->std::io::Result<()>{Ok(())}
}
fn observe_inner<F,P>(phase:&str,encode:F,publish:P)->bool where F:FnOnce(&str,&mut ObservationWriter)->Result<(),serde_json::Error>,P:FnOnce(&[u8])->std::io::Result<()> {
    let Ok(_guard)=EMITTER.lock()else{return false;};
    let mut writer=ObservationWriter {bytes:Vec::new()};
    if encode(phase,&mut writer).is_err() || writer.write_all(b"\n").is_err(){
        // No partial record is published; independent full diagnostics remain.
        writer.bytes.clear();
        let unavailable=Observation {version:1,phase,event:"observer-unavailable",payload:Fields(Field("reason","observation allocation or serialization limit",()))};
        if serde_json::to_writer(&mut writer,&unavailable).is_err() || writer.write_all(b"\n").is_err(){return false;}
    }
    publish(&writer.bytes).is_ok()
    // Writer and all borrowed field temporaries die before the mutex guard.
}
pub(super) fn observe<F>(encode:F) where F:FnOnce(&str,&mut ObservationWriter)->Result<(),serde_json::Error>{
    if !active(){return;}
    let phase=if FOCUS.load(Ordering::Relaxed){"focus"}else{"full"};
    observe_inner(phase,encode,|bytes|{
        // Even path copying/opening happens under the single observer owner.
        let path=std::env::var_os("SAIL_ENGINE_EVENTS").ok_or_else(||std::io::Error::other("observation path unavailable"))?;
        let mut out=std::fs::OpenOptions::new().create(true).append(true).open(path)?;out.write_all(bytes)
    });
}
macro_rules! fields {()=>{()};($key:literal:$value:expr $(,$tail_key:literal:$tail_value:expr)* $(,)?)=>{$crate::verifier::sail_engine::Field($key,&$value,$crate::verifier::sail_engine::fields!($($tail_key:$tail_value),*))};}
pub(super) use fields;
macro_rules! emit {($kind:expr,{$($key:literal:$value:expr),* $(,)?})=>{$crate::verifier::sail_engine::observe(|phase,writer|serde_json::to_writer(writer,&$crate::verifier::sail_engine::Observation {version:1,phase,event:$kind,payload:$crate::verifier::sail_engine::Fields($crate::verifier::sail_engine::fields!($($key:$value),*))}))};}
pub(super) use emit;
pub(super) fn diagnostic(message:&ArcDynMessage,level:MessageLevel){if let Some(msg)=message.downcast_ref::<MessageX>(){emit!("diagnostic",{"message":msg,"level":level});}}
struct Ordinals {values:HashMap<(bool,String,String),u32>,charged:usize}
static ORDINALS:std::sync::Mutex<Option<Ordinals>>=std::sync::Mutex::new(None);
fn ordinal(function:&str,kind:&str)->Option<u32>{
    let mut slot=ORDINALS.lock().ok()?;let state=slot.get_or_insert_with(||Ordinals {values:HashMap::new(),charged:0});
    // Admission covers retained keys and HashMap growth/old-new table overlap.
    // No unbounded key is ever cloned; the input owner is separately admitted.
    if function.len()>OBSERVER_NAMES || kind.len()>64{return None;}
    let key=(FOCUS.load(Ordering::Relaxed),function.to_owned(),kind.to_owned());
    if let Some(count)=state.values.get_mut(&key){let current=*count;*count=count.checked_add(1)?;return Some(current);}
    let charge=function.len().checked_add(kind.len())?.checked_mul(8)?.checked_add(std::mem::size_of::<(bool,String,String)>().checked_mul(8)?)?.checked_add(512)?;
    let total=state.charged.checked_add(charge)?;if total>2*1024*1024{return None;}
    state.values.try_reserve(1).ok()?;state.values.insert(key,1);state.charged=total;Some(0)
}
// Friendly-name multiplicity of one crate, built once per crate. Scanning every
// function per obligation made progress observation quadratic and serialized
// all verification workers on the global friendly-name lock.
const UNIQUE_NAMES_BYTES:usize=64*1024*1024;
static UNIQUE_NAMES:std::sync::Mutex<Option<(usize,Option<HashMap<String,u32>>)>>=std::sync::Mutex::new(None);
fn unique_names(krate:&Krate)->Option<HashMap<String,u32>>{
    let mut names=HashMap::new();let mut bytes=0usize;
    for function in krate.functions.iter(){
        let name=vir::ast_util::sail_bounded_friendly_name(&function.x.name.path,OBSERVER_NAMES)?;
        bytes=bytes.checked_add(name.capacity())?.checked_add(64)?;if bytes>UNIQUE_NAMES_BYTES{return None;}
        let count=names.entry(name).or_insert(0u32);*count=count.saturating_add(1);
    }
    Some(names)
}
fn unique(v:&Verifier,fun:&Fun)->bool {
    let Some(name)=vir::ast_util::sail_bounded_friendly_name(&fun.path,OBSERVER_NAMES)else{return false;};
    let Some(krate)=v.vir_crate.as_ref()else{return false;};
    let key=Arc::as_ptr(krate) as usize;
    let Ok(mut slot)=UNIQUE_NAMES.lock()else{return false;};
    if slot.as_ref().is_none_or(|(owner,_)|*owner!=key){*slot=Some((key,unique_names(krate)));}
    // An unavailable table (name or size budget) reports nothing as unique.
    slot.as_ref().and_then(|(_,names)|names.as_ref()).and_then(|names|names.get(&name)).copied()==Some(1)
}
pub(super) fn progress_start(v:&Verifier,fun:&Fun,comment:&str,description:&str,spinoff:Option<usize>,checks:usize)->Option<Progress> {
    if !active(){return None;}
    let Some(owner)=ObserverOwner::acquire()else{emit!("observer-unavailable",{"reason":"progress owner budget"});return None;};
    if !unique(v,fun){return None;}
    let function=vir::ast_util::sail_bounded_friendly_name(&fun.path,OBSERVER_NAMES)?;
    let kind=comment.split_once(' ')?.0;
    if description.len()>4096 || kind.len()>64{return None;}
    emit!("function-started",{"function":function});
    let identity=if let Some(index)=spinoff {
        if checks!=1{return None;}format!("{function}#{kind}/spinoff-{index}")
    }else{let Some(count)=ordinal(&function,kind)else{emit!("observer-unavailable",{"reason":"progress ordinal budget"});return None;};format!("{function}#{kind}/{count}")};
    emit!("obligation-progress",{"function":function,"obligation":identity,"description":description,"completed":false});
    Some(Progress {identity:Some(identity),_owner:owner})
}
pub(super) fn functions_observed(v:&Verifier,bucket:&BucketId) {
    if !active(){return;}
    if let Some(stats)=v.func_times.get(bucket){for (fun,stats) in stats {
        let Some(_owner)=ObserverOwner::acquire()else{emit!("observer-unavailable",{"reason":"progress owner budget"});continue;};
        if unique(v,fun) && v.user_filter.as_ref().is_some_and(|filter|filter.includes_function(fun)) {
            emit!("function-observed",{"function":ObservedFriendly(fun),"module":ObservedModule(bucket.module()),"mode":v.get_function_mode(fun).map(|mode|mode.to_string()),"success":!v.func_fails.contains(fun),"smt_micros":stats.smt_time.as_micros(),"rlimit":stats.rlimit_count});
        }
    }}
}
fn clear() { if let Ok(mut ordinals)=ORDINALS.lock(){*ordinals=None;} if let Ok(mut names)=UNIQUE_NAMES.lock(){*names=None;} }
fn dispose_report_messages(messages:&mut RefCell<Messages>,charge:u64){
    // Release every retained Arc and the Vec allocation before quota becomes
    // available to a concurrent reporter. Compiler-owned clones remain outside
    // focused ownership and are never counted as a retained allocation.
    drop(std::mem::take(messages.get_mut()));
    REPORT_BYTES.fetch_sub(charge,Ordering::Relaxed);
}
fn message_charge(msg:&ArcDynMessage)->Option<u64>{message_x_charge(msg.downcast_ref::<MessageX>()?)}
fn message_x_charge(msg:&MessageX)->Option<u64>{
    fn span_charge(span:&vir::messages::Span)->Option<usize>{
        let raw=(&*span.raw_span) as &(dyn std::any::Any+Send+Sync);
        let raw_size=if raw.is::<()>(){0}else if raw.is::<rustc_span::SpanData>(){std::mem::size_of::<rustc_span::SpanData>()}else{return None;};
        Some(span.data.capacity().checked_mul(8)?.checked_add(span.as_string.capacity())?.checked_add(raw_size)?.checked_add(64)?)
    }
    // Count reachable allocations even when the original reporter releases its Arc.
    // Duplicate/shared spans are deliberately overcharged; unknown raw owners refuse.
    let mut charge=std::mem::size_of::<MessageX>().checked_add(128)?.checked_add(msg.note.capacity())?
        .checked_add(msg.spans.capacity().checked_mul(std::mem::size_of::<vir::messages::Span>())?)?
        .checked_add(msg.labels.capacity().checked_mul(std::mem::size_of::<vir::messages::MessageLabel>())?)?
        .checked_add(msg.help.capacity().checked_mul(std::mem::size_of::<String>())?)?;
    if let Some(note)=&msg.fancy_note{charge=charge.checked_add(note.capacity())?;}
    for span in &msg.spans{charge=charge.checked_add(span_charge(span)?)?;}
    for label in &msg.labels{charge=charge.checked_add(label.note.capacity())?.checked_add(span_charge(&label.span)?)?;}
    for help in &msg.help{charge=charge.checked_add(help.capacity())?;}
    u64::try_from(charge).ok()
}
fn admit_message(msg:&ArcDynMessage)->Option<u64>{
    let charge=message_charge(msg)?;
    REPORT_BYTES.try_update(Ordering::Relaxed,Ordering::Relaxed,|used|used.checked_add(charge).filter(|used|*used<=16*1024*1024)).ok()?;
    Some(charge)
}
#[derive(Default)]
struct FocusReporter { messages: RefCell<Messages>,charge:Cell<u64>,incomplete:Cell<bool> }
impl Drop for FocusReporter{fn drop(&mut self){dispose_report_messages(&mut self.messages,self.charge.get());}}
impl air::messages::Diagnostics for FocusReporter {
    fn report_as(&self,msg:&ArcDynMessage,level:MessageLevel){ if let Some(charge)=admit_message(msg){self.charge.set(self.charge.get()+charge);self.messages.borrow_mut().push((msg.clone(),level));}else{self.incomplete.set(true);}diagnostic(msg,level); }
    fn report(&self,msg:&ArcDynMessage){self.report_as(msg,msg.downcast_ref::<MessageX>().unwrap().level);}
    fn report_now(&self,msg:&ArcDynMessage){self.report(msg);}
    fn report_as_now(&self,msg:&ArcDynMessage,level:MessageLevel){self.report_as(msg,level);}
}
impl Diagnostics for FocusReporter {
    fn use_progress_bars(&self)->bool{false}
    fn add_progress_bar(&self,_:CommandContext){}
    fn complete_progress_bar(&self,_:CommandContext){}
}
const FOCUS_METADATA:usize=8*1024*1024;
const FOCUS_NAME:usize=1024*1024;
struct FocusMetadata {used:usize}
impl FocusMetadata {
    fn charge(&mut self,bytes:usize)->Option<()> {
        self.used=self.used.checked_add(bytes)?;if self.used>FOCUS_METADATA{return None;}Some(())
    }
}
#[derive(serde::Serialize)]
struct FocusDiagnostic<'a>{message:&'a MessageX,level:MessageLevel}
struct FocusDiagnostics<'a>(&'a Messages);
impl serde::Serialize for FocusDiagnostics<'_> {
    fn serialize<S:serde::Serializer>(&self,serializer:S)->Result<S::Ok,S::Error>{
        use serde::ser::SerializeSeq;
        let mut seq=serializer.serialize_seq(Some(self.0.len()))?;
        for (message,level) in self.0 {let message=message.downcast_ref::<MessageX>().ok_or_else(||serde::ser::Error::custom("unsupported focused diagnostic owner"))?;seq.serialize_element(&FocusDiagnostic {message,level:*level})?;}
        seq.end()
    }
}
#[derive(serde::Serialize)]
struct FocusFunctionTime {function:String,#[serde(rename="mode:")] mode:Option<String>,#[serde(rename="time-micros")] time_micros:u128,rlimit:Option<u64>,success:bool}
#[derive(serde::Serialize)]
struct FocusModuleTime {module:String,#[serde(rename="function-breakdown")] functions:Vec<FocusFunctionTime>}
#[derive(serde::Serialize)]
struct FocusSmt {#[serde(rename="smt-run-module-times")] times:Vec<FocusModuleTime>}
#[derive(serde::Serialize)]
struct FocusTimes {smt:FocusSmt}
#[derive(serde::Serialize)]
struct FocusResults {#[serde(rename="encountered-error")] encountered_error:bool,#[serde(rename="encountered-vir-error")] encountered_vir_error:bool,verified:u64,errors:u64,#[serde(rename="is-verifying-entire-crate")] entire:bool}
#[derive(serde::Serialize)]
struct FocusVerus<'a> {#[serde(rename="func-details")] functions:std::collections::BTreeMap<String,&'a FuncDetails>,#[serde(rename="verification-results")] results:FocusResults,#[serde(rename="times-ms")] times:FocusTimes}
#[derive(serde::Serialize)]
struct FocusDocument<'a> {version:u32,function:&'a str,confirmed:bool,diagnostics:FocusDiagnostics<'a>,verus:FocusVerus<'a>}
fn bounded_module_name(path:&vir::ast::Path,limit:usize)->Option<String>{
    let mut bytes=0usize;for segment in path.segments.iter(){bytes=bytes.checked_add(segment.len())?.checked_add(2)?;}
    if bytes>limit{return None;}
    let mut name=String::new();name.try_reserve_exact(bytes).ok()?;if name.capacity()>limit{return None;}
    for (index,segment) in path.segments.iter().enumerate(){if index!=0{name.push_str("::");}name.push_str(segment);}
    Some(name)
}
fn focused_document(focus:&Verifier,name:&str,messages:&Messages,failed:bool)->Option<Vec<u8>>{
    let mut metadata=FocusMetadata {used:0};
    let mut functions=std::collections::BTreeMap::new();
    for (fun,details) in &focus.func_details {
        // Only the selected function: other friendly names may repeat (trait impls).
        let Some(function)=vir::ast_util::sail_bounded_friendly_name(&fun.path,FOCUS_NAME).filter(|function|function==name) else {continue;};
        metadata.charge(function.capacity().checked_add(1024)?)?;
        if functions.insert(function,details).is_some(){return None;}
    }
    let mut times=Vec::new();
    for (bucket,stats) in &focus.func_times {
        let module=bounded_module_name(bucket.module(),FOCUS_NAME)?;metadata.charge(module.capacity().checked_add(1024)?)?;
        let mut functions=Vec::new();
        for (fun,stats) in stats {
            let function=vir::ast_util::sail_bounded_friendly_name(&fun.path,FOCUS_NAME)?;
            metadata.charge(function.capacity().checked_add(1024)?)?;
            functions.push(FocusFunctionTime {function,mode:focus.get_function_mode(fun).map(|mode|mode.to_string()),time_micros:stats.smt_time.as_micros(),rlimit:stats.rlimit_count,success:!focus.func_fails.contains(fun)});
        }
        times.push(FocusModuleTime {module,functions});
    }
    let document=FocusDocument {version:1,function:name,confirmed:false,diagnostics:FocusDiagnostics(messages),verus:FocusVerus {functions,results:FocusResults {encountered_error:failed,encountered_vir_error:focus.encountered_vir_error,verified:focus.count_verified,errors:focus.count_errors,entire:false},times:FocusTimes {smt:FocusSmt {times}}}};
    let mut writer=Bounded {bytes:Vec::new()};
    serde_json::to_writer(&mut writer,&document).ok()?;Some(writer.bytes)
}
impl Verifier {
    pub(super) fn sail_focus(&mut self, compiler: &Compiler, spans:&SpanContext) {
        if !active() { return; }
        clear();
        let Ok(name)=std::env::var("SAIL_ENGINE_FUNCTION") else { return; };
        let krate=self.vir_crate.as_ref().unwrap();
        if name.len()>FOCUS_NAME {emit!("focus-unavailable", {"reason":"focused name budget"});return;}
        let mut selected=None;let mut count=0usize;
        for function in &krate.functions {if vir::ast_util::sail_bounded_friendly_name(&function.x.name.path,FOCUS_NAME).as_deref()==Some(name.as_str()){count=count.saturating_add(1);selected=Some(function);}}
        if count!=1 {emit!("focus-unavailable", {"function":name,"matches":count});return;}
        let Some(selected)=selected else{return;};
        let module=selected.x.owning_module.clone().unwrap();
        let mut focus=self.from_self();
        focus.user_filter=Some(UserFilter::Function(module.segments.clone(),name.clone(),HashSet::from([selected.x.name.clone()])));
        let log=self.args.log_dir.as_ref().map(|p|std::path::PathBuf::from(p).join("focus"));
        if let Some(path)=log { let _=std::fs::create_dir_all(&path);focus.created_log_dir=Arc::new(std::sync::Mutex::new(Some(path.clone())));focus.created_solver_log_dir=Arc::new(std::sync::Mutex::new(Some(path))); }
        FOCUS.store(true,Ordering::Relaxed);
        emit!("phase-started", {"function":name});
        let reporter=FocusReporter::default();
        let status=focus.verify_crate_inner(compiler,spans,&reporter);
        if reporter.incomplete.get(){emit!("focus-unavailable", {"function":name,"reason":"focus diagnostic retention budget"});}
        else if let Some(document)=focused_document(&focus,&name,&reporter.messages.borrow(),status.is_err()) {
            if let Some(path)=std::env::var_os("SAIL_ENGINE_FOCUS_REPORT"){let _=std::fs::write(path,&document);}
        }else{emit!("focus-unavailable", {"function":name,"reason":"focused report allocation budget"});}
        emit!("phase-finished", {"function":name,"errors":focus.count_errors,"confirmed":false});
        FOCUS.store(false,Ordering::Relaxed);
        emit!("phase-started", {});
    }
}

#[cfg(test)] mod reporter_ownership_tests {
    use super::*;
    use std::sync::{mpsc,Mutex};
    struct BlockedSpan { bytes:Option<Vec<u8>>, started:mpsc::Sender<()>, finish:Mutex<mpsc::Receiver<()>> }
    impl Drop for BlockedSpan {
        fn drop(&mut self){self.started.send(()).unwrap();self.finish.get_mut().unwrap().recv().unwrap();drop(self.bytes.take());}
    }
    #[test]
    fn focus_reporter_disposes_nested_ownership_before_racing_admission(){
        const LIMIT:u64=16*1024*1024;
        assert_eq!(REPORT_BYTES.fetch_add(LIMIT,Ordering::SeqCst),0);
        let (started_tx,started_rx)=mpsc::channel();let (finish_tx,finish_rx)=mpsc::channel();
        let span=vir::messages::Span{raw_span:Arc::new(BlockedSpan{bytes:Some(vec![0;128]),started:started_tx,finish:Mutex::new(finish_rx)}),id:0,data:vec![],as_string:String::new()};
        let message:ArcDynMessage=vir::messages::error(&span,"nested retained diagnostic");drop(span);
        let worker=std::thread::spawn(move||{
            let messages=RefCell::new(vec![(message,MessageLevel::Error)]);
            drop(FocusReporter{messages,charge:Cell::new(LIMIT),incomplete:Cell::new(false)});
        });
        started_rx.recv().unwrap();
        assert!(REPORT_BYTES.try_update(Ordering::SeqCst,Ordering::SeqCst,|used|used.checked_add(1).filter(|total|*total<=LIMIT)).is_err(),"admitted new diagnostic while old nested span allocation is still live");
        finish_tx.send(()).unwrap();worker.join().unwrap();
        assert_eq!(REPORT_BYTES.load(Ordering::SeqCst),0);
    }
    #[test]
    fn disposal_releases_only_retained_references(){
        let message:ArcDynMessage=vir::messages::error_bare("compiler-owned reference remains valid");
        let reporter=FocusReporter{messages:RefCell::new(vec![(message.clone(),MessageLevel::Error)]),charge:Cell::new(0),incomplete:Cell::new(false)};
        assert_eq!(Arc::strong_count(&message),2);drop(reporter);assert_eq!(Arc::strong_count(&message),1);
        assert_eq!(message.downcast_ref::<MessageX>().unwrap().note,"compiler-owned reference remains valid");
    }
}

#[cfg(test)]
mod focused_serialization_tests {
    use super::*;
    #[test]
    fn bounded_borrowed_report_preserves_the_complete_legacy_json_graph() {
        let message:ArcDynMessage=vir::messages::error_bare("exact diagnostic α\n\"escaped\"");
        let messages=vec![(message,MessageLevel::Error)];
        let details=FuncDetails {obligation_proof_notes:HashSet::from(["obligation exact".to_owned()]),failed_proof_notes:HashSet::new()};
        let functions=std::collections::BTreeMap::from([("fixture::target".to_owned(),&details)]);
        let document=FocusDocument {version:1,function:"fixture::target",confirmed:false,diagnostics:FocusDiagnostics(&messages),verus:FocusVerus {functions,results:FocusResults {encountered_error:false,encountered_vir_error:false,verified:1,errors:0,entire:false},times:FocusTimes {smt:FocusSmt {times:vec![FocusModuleTime {module:"fixture".to_owned(),functions:vec![FocusFunctionTime {function:"fixture::target".to_owned(),mode:Some("proof".to_owned()),time_micros:17,rlimit:Some(29),success:true}]}]}}}};
        let expected=serde_json::json!({"version":1,"function":"fixture::target","confirmed":false,"diagnostics":[{"message":messages[0].0.downcast_ref::<MessageX>().unwrap(),"level":MessageLevel::Error}],"verus":{"func-details":{"fixture::target":details},"verification-results":{"encountered-error":false,"encountered-vir-error":false,"verified":1,"errors":0,"is-verifying-entire-crate":false},"times-ms":{"smt":{"smt-run-module-times":[{"module":"fixture","function-breakdown":[{"function":"fixture::target","mode:":"proof","time-micros":17,"rlimit":29,"success":true}]}]}}}});
        let mut writer=Bounded {bytes:Vec::new()};serde_json::to_writer(&mut writer,&document).unwrap();
        assert_eq!(serde_json::from_slice::<serde_json::Value>(&writer.bytes).unwrap(),expected);
        assert_eq!(Arc::strong_count(&messages[0].0),1,"serialization must not retain another diagnostic owner");
        let mut metadata=FocusMetadata {used:FOCUS_METADATA};assert!(metadata.charge(1).is_none());
        let mut writer=Bounded {bytes:vec![0;BUFFER_BUDGET]};assert!(writer.write(&[1]).is_err());assert_eq!(writer.bytes.len(),BUFFER_BUDGET);
    }
}

#[cfg(test)] mod bounded_observer_tests {
    use super::*;
    #[test] fn borrowed_map_and_diagnostic_shape_matches_the_existing_bytes(){
        let message=Arc::new("exact α\n\"diagnostic\"".to_owned());
        let mut bytes=Vec::new();
        assert!(observe_inner("focus",|phase,writer|serde_json::to_writer(writer,&Observation {version:1,phase,event:"diagnostic",payload:Fields(Field("message",&*message,Field("level","error",())))}),|frame|{bytes.extend_from_slice(frame);Ok(())}));
        let expected=serde_json::json!({"version":1,"phase":"focus","event":"diagnostic","payload":{"message":&*message,"level":"error"}});
        let mut legacy=serde_json::to_vec(&expected).unwrap();legacy.push(b'\n');assert_eq!(bytes,legacy);assert_eq!(Arc::strong_count(&message),1);
        let mut bytes=Vec::new();assert!(observe_inner("full",|phase,writer|serde_json::to_writer(writer,&Observation {version:1,phase,event:"compiler-sealed",payload:Fields(Field("success",true,Field("normal_termination",true,())))}),|frame|{bytes.extend_from_slice(frame);Ok(())}));
        let parsed:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(parsed,serde_json::json!({"version":1,"phase":"full","event":"compiler-sealed","payload":{"success":true,"normal_termination":true}}));
    }
    #[test] fn real_diagnostic_observer_retains_the_entire_message_graph_without_cloning() {
        let span=vir::messages::Span {raw_span:Arc::new(()),id:19,data:vec![7,11],as_string:"exact source α:12:8".to_owned()};
        let message:ArcDynMessage=vir::messages::error(&span,"exact α\n\"diagnostic\"");let raw=message.downcast_ref::<MessageX>().unwrap();
        let mut bytes=Vec::new();assert!(observe_inner("focus",|phase,writer|serde_json::to_writer(writer,&Observation {version:1,phase,event:"diagnostic",payload:Fields(Field("message",raw,Field("level",MessageLevel::Error,())))}),|frame|{bytes.extend_from_slice(frame);Ok(())}));
        assert_eq!(serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),serde_json::json!({"version":1,"phase":"focus","event":"diagnostic","payload":{"message":raw,"level":MessageLevel::Error}}));assert_eq!(Arc::strong_count(&message),1);
    }
    #[test] fn excess_escaping_publishes_only_unavailable_and_preserves_borrowed_owner(){
        let message=Arc::new("\n".repeat(OBSERVER_BYTES));let original=message.len();let mut bytes=Vec::new();
        assert!(observe_inner("focus",|phase,writer|serde_json::to_writer(writer,&Observation {version:1,phase,event:"diagnostic",payload:Fields(Field("message",&*message,()))}),|frame|{bytes.extend_from_slice(frame);Ok(())}));
        let parsed:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(parsed["event"],"observer-unavailable");assert!(bytes.len()<512);assert_eq!(message.len(),original);assert_eq!(Arc::strong_count(&message),1);
        assert!(!observe_inner("full",|phase,writer|serde_json::to_writer(writer,&Observation {version:1,phase,event:"diagnostic",payload:Fields(Field("message",&*message,()))}),|_|Err(std::io::Error::other("owned write failure"))));
        let mut writer=ObservationWriter {bytes:Vec::new()};assert!(writer.write(&vec![0;OBSERVER_BYTES+1]).is_err());assert_eq!(writer.bytes.capacity(),0);
    }
    #[test] fn duplicate_fields_cannot_publish_a_partial_diagnostic_or_seal(){
        let mut bytes=Vec::new();assert!(observe_inner("full",|phase,writer|serde_json::to_writer(writer,&Observation {version:1,phase,event:"compiler-sealed",payload:Fields(Field("success",true,Field("success",false,())))}),|frame|{bytes.extend_from_slice(frame);Ok(())}));
        let parsed:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(parsed["event"],"observer-unavailable");assert!(parsed["payload"].get("success").is_none());assert_eq!(bytes.iter().filter(|byte|**byte==b'\n').count(),1);
    }
    #[test] fn field_conversion_waits_for_single_observer_ownership(){
        let (owned_tx,owned_rx)=std::sync::mpsc::channel();let (finish_tx,finish_rx)=std::sync::mpsc::channel();
        let first=std::thread::spawn(move||observe_inner("full",|_,_|{owned_tx.send(()).unwrap();finish_rx.recv().unwrap();Ok(())},|_|Ok(())));
        owned_rx.recv().unwrap();let entered=Arc::new(AtomicBool::new(false));let second_entered=entered.clone();let (queued_tx,queued_rx)=std::sync::mpsc::channel();
        let second=std::thread::spawn(move||{queued_tx.send(()).unwrap();observe_inner("full",|_,_|{second_entered.store(true,Ordering::SeqCst);Ok(())},|_|Ok(()))});
        queued_rx.recv().unwrap();assert!(!entered.load(Ordering::SeqCst));finish_tx.send(()).unwrap();assert!(first.join().unwrap());assert!(second.join().unwrap());assert!(entered.load(Ordering::SeqCst));
    }
    #[test] fn progress_owners_and_ordinals_refuse_excess_and_release_after_drop(){
        let mut owners=Vec::new();while let Some(owner)=ObserverOwner::acquire(){owners.push(owner);}assert!(OBSERVER_OWNED.load(Ordering::SeqCst)<=OBSERVER_OWNERS as u64);assert!(ObserverOwner::acquire().is_none());
        let owner=owners.pop().unwrap();let identity="retained-obligation".to_owned();let progress=Progress {identity:Some(identity),_owner:owner};let before=OBSERVER_OWNED.load(Ordering::SeqCst);assert_eq!(serde_json::to_string(&progress).unwrap(),"\"retained-obligation\"");drop(progress);assert_eq!(OBSERVER_OWNED.load(Ordering::SeqCst),before-24*1024);drop(owners);assert_eq!(OBSERVER_OWNED.load(Ordering::SeqCst),0);
        {let mut slot=ORDINALS.lock().unwrap();*slot=Some(Ordinals {values:HashMap::new(),charged:2*1024*1024});}assert!(ordinal("fixture::target","Function-Def").is_none());assert!(ORDINALS.lock().unwrap().as_ref().unwrap().values.is_empty());*ORDINALS.lock().unwrap()=None;
        assert_eq!(ordinal("fixture::target","Function-Def"),Some(0));assert_eq!(ordinal("fixture::target","Function-Def"),Some(1));*ORDINALS.lock().unwrap()=None;
    }
}
