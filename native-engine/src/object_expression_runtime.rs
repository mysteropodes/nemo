//! Default-off, private Order expression execution. No document or C1 writer.
//! A returned candidate owns only Rust data; every JS handle is retired first.
use crate::history::NativeObjectHistory;
use crate::object_expression_pin::{
    prepare_expression_request, ExpressionRequest, ExpressionSelector,
};
use crate::object_frame_packet::ObjectFrameError;
use crate::revision::ObjectSnapshot;
use rquickjs::{context::intrinsic, Context, Ctx, Error, Function, Runtime, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

/// Bounded values must be chosen explicitly by the caller for its workload.
/// The private feature has no production default or public host route.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ExpressionLimits {
    input_bytes: usize,
    heap_bytes: usize,
    stack_bytes: usize,
    interrupt_polls: usize,
}

impl ExpressionLimits {
    // Conservative private-stage ceilings from the independently accepted
    // ORIGINAL39 local envelope. They are not a desktop workload default;
    // a later host must justify each lower/equal choice before activation.
    const MAX_INPUT_BYTES: usize = 4096;
    const MAX_HEAP_BYTES: usize = 8_388_608;
    const MAX_STACK_BYTES: usize = 262_144;
    const MAX_INTERRUPT_POLLS: usize = 32;

    pub(crate) fn new(
        input_bytes: usize,
        heap_bytes: usize,
        stack_bytes: usize,
        interrupt_polls: usize,
    ) -> Result<Self, ExpressionRefusal> {
        if input_bytes == 0
            || input_bytes > Self::MAX_INPUT_BYTES
            || heap_bytes == 0
            || heap_bytes > Self::MAX_HEAP_BYTES
            || stack_bytes == 0
            || stack_bytes > Self::MAX_STACK_BYTES
            || interrupt_polls == 0
            || interrupt_polls > Self::MAX_INTERRUPT_POLLS
        {
            return Err(ExpressionRefusal::InvalidLimits);
        }
        Ok(Self {
            input_bytes,
            heap_bytes,
            stack_bytes,
            interrupt_polls,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExpressionRefusal {
    Pin(ObjectFrameError),
    InvalidLimits,
    InputLimit,
    RuntimeUnavailable,
    ContextUnavailable,
    Bootstrap,
    Syntax,
    Guest,
    Interrupted,
    Stack,
    Memory,
    NonPrimitive,
    NonFinite,
    InitialQueue,
    QueuedAsyncWork,
    RuntimeOwnerRetained,
}

impl From<ObjectFrameError> for ExpressionRefusal {
    fn from(value: ObjectFrameError) -> Self {
        Self::Pin(value)
    }
}

/// Deliberately opaque: the sole scalar extraction is the pin module's finalizer.
pub(crate) struct DetachedCandidate {
    outcome: Result<f64, ExpressionRefusal>,
}

impl DetachedCandidate {
    pub(super) fn into_outcome(self) -> Result<f64, ExpressionRefusal> {
        self.outcome
    }
}

// Promise is installed for internal async classes and job detection, then
// hidden from guest globals along with constructors and host-like capabilities.
const HARDEN: &str = r#"(() => {
'use strict';
const internalPromise = Promise;
const fnProtos = [Function.prototype,
 Object.getPrototypeOf(function*(){}), Object.getPrototypeOf(async function(){}),
 Object.getPrototypeOf(async function*(){})];
for (const p of [...fnProtos, internalPromise.prototype])
 Object.defineProperty(p, 'constructor', {value:undefined,writable:false,configurable:false});
Object.defineProperty(Math, 'random', {value:undefined,writable:false,configurable:false});
if (typeof Array.fromAsync !== 'undefined')
 Object.defineProperty(Array, 'fromAsync', {value:undefined,writable:false,configurable:false});
for (const n of ['eval','Function','Date','performance','Promise','SharedArrayBuffer','Atomics',
 'WebAssembly','WeakRef','FinalizationRegistry','Proxy','window','document','process','require',
 'fetch','XMLHttpRequest','WebSocket','console','setTimeout','queueMicrotask','Deno','Bun'])
 Object.defineProperty(globalThis,n,{value:undefined,writable:false,configurable:false});
const seen=[];
function freeze(o) {
 if(o===null || (typeof o!=='object' && typeof o!=='function') || seen.indexOf(o)>=0) return;
 seen.push(o);
 for(const k of Object.getOwnPropertyNames(o)) {
  const d=Object.getOwnPropertyDescriptor(o,k);
  if(d && 'value' in d) freeze(d.value);
  if(d && d.get) freeze(d.get); if(d && d.set) freeze(d.set);
 }
 const p=Object.getPrototypeOf(o); if(p) freeze(p);
 Object.freeze(o);
}
for(const p of fnProtos) freeze(p);
freeze(internalPromise);
freeze(globalThis);
return 1;
})()"#;

fn guest_error(ctx: &Ctx<'_>, error: Error, polls: usize, budget: usize) -> ExpressionRefusal {
    if polls >= budget {
        return ExpressionRefusal::Interrupted;
    }
    if matches!(error, Error::Allocation) {
        return ExpressionRefusal::Memory;
    }
    if !matches!(error, Error::Exception) {
        return ExpressionRefusal::Guest;
    }
    let exception = ctx.catch();
    let Some(object) = exception.as_object() else {
        return ExpressionRefusal::Guest;
    };
    let name = object.get::<_, String>("name").unwrap_or_default();
    let message = object
        .get::<_, String>("message")
        .unwrap_or_default()
        .to_lowercase();
    if message.contains("stack") {
        ExpressionRefusal::Stack
    } else if message.contains("memory") || message.contains("alloc") {
        ExpressionRefusal::Memory
    } else if name == "SyntaxError" {
        ExpressionRefusal::Syntax
    } else {
        ExpressionRefusal::Guest
    }
}

fn guest_result(value: Value<'_>) -> Result<f64, ExpressionRefusal> {
    if let Some(number) = value.as_number() {
        return if number.is_finite() {
            Ok(number)
        } else {
            Err(ExpressionRefusal::NonFinite)
        };
    }
    Err(ExpressionRefusal::NonPrimitive)
}

fn evaluate_in_context(
    rt: &Runtime,
    request: &ExpressionRequest,
    limits: ExpressionLimits,
) -> Result<f64, ExpressionRefusal> {
    let context = Context::custom::<(
        intrinsic::Eval,
        intrinsic::RegExpCompiler,
        intrinsic::RegExp,
        intrinsic::Promise,
    )>(rt)
    .map_err(|_| ExpressionRefusal::ContextUnavailable)?;
    let bootstrap = context.with(|ctx| ctx.eval::<i32, _>(HARDEN));
    if bootstrap.is_err() {
        // Consume the pending exception while its context is still alive.
        context.with(|ctx| {
            let _ = ctx.catch();
        });
        drop(context);
        rt.run_gc();
        return Err(ExpressionRefusal::Bootstrap);
    }
    let polls = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&polls);
    rt.set_interrupt_handler(Some(Box::new(move || {
        seen.fetch_add(1, Ordering::SeqCst) + 1 >= limits.interrupt_polls
    })));
    let result = context.with(|ctx| {
        let expression = format!(
            "(function(value,frame){{'use strict';return ({}\n);}})",
            request.code()
        );
        let function: Function = match ctx.eval(expression) {
            Ok(function) => function,
            Err(error) => {
                let refusal = guest_error(
                    &ctx,
                    error,
                    polls.load(Ordering::SeqCst),
                    limits.interrupt_polls,
                );
                if refusal != ExpressionRefusal::Syntax {
                    return Err(refusal);
                }
                let body = format!(
                    "(function(value,frame){{'use strict';{}\n}})",
                    request.code()
                );
                ctx.eval(body).map_err(|error| {
                    guest_error(
                        &ctx,
                        error,
                        polls.load(Ordering::SeqCst),
                        limits.interrupt_polls,
                    )
                })?
            }
        };
        let value: Value = function
            .call((request.raw_numeric(), request.evaluated_frame()))
            .map_err(|error| {
                guest_error(
                    &ctx,
                    error,
                    polls.load(Ordering::SeqCst),
                    limits.interrupt_polls,
                )
            })?;
        guest_result(value)
    });
    rt.set_interrupt_handler(None);
    drop(context);
    rt.run_gc();
    result
}

/// Source-only stage: always one fresh Runtime; no job executor or loader.
pub(crate) fn evaluate_detached(
    request: &ExpressionRequest,
    limits: ExpressionLimits,
) -> Result<DetachedCandidate, ExpressionRefusal> {
    if request.code().len() > limits.input_bytes {
        return Err(ExpressionRefusal::InputLimit);
    }
    let rt = Runtime::new().map_err(|_| ExpressionRefusal::RuntimeUnavailable)?;
    rt.set_memory_limit(limits.heap_bytes);
    rt.set_max_stack_size(limits.stack_bytes);
    let weak = rt.weak();
    let initial_queue = rt.is_job_pending();
    let mut outcome = if initial_queue {
        Err(ExpressionRefusal::InitialQueue)
    } else {
        evaluate_in_context(&rt, request, limits)
    };
    let pending = rt.is_job_pending();
    if pending {
        outcome = Err(ExpressionRefusal::QueuedAsyncWork);
    }
    // The outcome above holds only Rust f64/error data; context and every JS
    // value were destroyed by evaluate_in_context before the sole Runtime drop.
    drop(rt);
    if weak.try_ref().is_some() {
        return Err(ExpressionRefusal::RuntimeOwnerRetained);
    }
    Ok(DetachedCandidate { outcome })
}

/// The only private scalar entry point: finalization rechecks the real owner.
pub(crate) fn evaluate_for_owner(
    owner: &NativeObjectHistory,
    snapshot: &ObjectSnapshot,
    selector: &ExpressionSelector<'_>,
    limits: ExpressionLimits,
) -> Result<f64, ExpressionRefusal> {
    let request = prepare_expression_request(owner, snapshot, selector)?;
    let candidate = match evaluate_detached(&request, limits) {
        Ok(candidate) => candidate,
        Err(refusal) => {
            // Even an early bounded refusal must not escape with an obsolete
            // owner pin; only detached Rust data may cross this boundary.
            request.revalidate(owner)?;
            return Err(refusal);
        }
    };
    request.finalize(owner, candidate)
}
