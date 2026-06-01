#![allow(dead_code)]

//! Experimental async + ALTREP scaffolding.
//!
//! This file is compiled only with `--features async-altrep`.  Background tasks
//! must never allocate or mutate R objects.  They return Rust-owned data.  ALTREP
//! `elt()`/`copy_to()` runs on the R main thread and converts Rust data to R.

use savvy::altrep::{
    AltInteger, AltList, AltString, register_altinteger_class, register_altlist_class,
    register_altstring_class,
};
use savvy::{IntoExtPtrSexp, OwnedIntegerSexp, OwnedStringSexp, Sexp, ffi::DllInfo};
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

type AsyncResult<T> = Result<T, String>;

fn background_runtime() -> savvy::Result<Arc<Runtime>> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .thread_name("Rzarrs-altrep-io")
        .build()
        .map(Arc::new)
        .map_err(|e| savvy::Error::new(&format!("cannot build ALTREP async runtime: {e}")))
}

pub struct AsyncI32Vec {
    runtime: Arc<Runtime>,
    len: usize,
    task: Option<JoinHandle<AsyncResult<Vec<i32>>>>,
    cache: Option<Vec<i32>>,
    error: Option<String>,
}

impl IntoExtPtrSexp for AsyncI32Vec {}

impl AsyncI32Vec {
    fn force(&mut self) -> &[i32] {
        if self.cache.is_none() {
            let task = self.task.take().expect("AsyncI32Vec task already consumed");
            match self.runtime.block_on(task) {
                Ok(Ok(v)) => self.cache = Some(v),
                Ok(Err(e)) => {
                    self.error = Some(e);
                    self.cache = Some(vec![i32::MIN; self.len]);
                }
                Err(e) => {
                    self.error = Some(e.to_string());
                    self.cache = Some(vec![i32::MIN; self.len]);
                }
            }
        }
        self.cache.as_deref().unwrap()
    }
}

impl AltInteger for AsyncI32Vec {
    const CLASS_NAME: &'static str = "AsyncI32Vec";
    const PACKAGE_NAME: &'static str = "Rzarrs";

    fn length(&mut self) -> usize {
        self.len
    }

    fn elt(&mut self, i: usize) -> i32 {
        self.force()[i]
    }

    fn copy_to(&mut self, out: &mut [i32], offset: usize) {
        let data = self.force();
        out.copy_from_slice(&data[offset..offset + out.len()]);
    }
}

pub struct AsyncStringVec {
    runtime: Arc<Runtime>,
    len: usize,
    task: Option<JoinHandle<AsyncResult<Vec<String>>>>,
    cache: Option<Vec<String>>,
    error: Option<String>,
}

impl IntoExtPtrSexp for AsyncStringVec {}

impl AsyncStringVec {
    fn force(&mut self) -> &[String] {
        if self.cache.is_none() {
            let task = self
                .task
                .take()
                .expect("AsyncStringVec task already consumed");
            match self.runtime.block_on(task) {
                Ok(Ok(v)) => self.cache = Some(v),
                Ok(Err(e)) => {
                    self.error = Some(e);
                    self.cache = Some(vec![String::new(); self.len]);
                }
                Err(e) => {
                    self.error = Some(e.to_string());
                    self.cache = Some(vec![String::new(); self.len]);
                }
            }
        }
        self.cache.as_deref().unwrap()
    }
}

impl AltString for AsyncStringVec {
    const CLASS_NAME: &'static str = "AsyncStringVec";
    const PACKAGE_NAME: &'static str = "Rzarrs";

    fn length(&mut self) -> usize {
        self.len
    }

    fn elt(&mut self, i: usize) -> &str {
        &self.force()[i]
    }
}

#[derive(Clone)]
pub enum LazyValue {
    I32(Vec<i32>),
    Utf8(Vec<String>),
}

impl LazyValue {
    fn to_sexp(&self) -> Sexp {
        match self {
            LazyValue::I32(v) => {
                let mut out =
                    OwnedIntegerSexp::new(v.len()).expect("cannot allocate integer vector");
                for (i, x) in v.iter().enumerate() {
                    out[i] = *x;
                }
                out.into()
            }
            LazyValue::Utf8(v) => {
                let mut out = OwnedStringSexp::new(v.len()).expect("cannot allocate string vector");
                for (i, x) in v.iter().enumerate() {
                    out.set_elt(i, x).expect("cannot set string element");
                }
                out.into()
            }
        }
    }
}

pub struct AsyncFieldList {
    runtime: Arc<Runtime>,
    names: Vec<String>,
    tasks: Vec<Option<JoinHandle<AsyncResult<LazyValue>>>>,
    cache: Vec<Option<LazyValue>>,
}

impl IntoExtPtrSexp for AsyncFieldList {}

impl AltList for AsyncFieldList {
    const CLASS_NAME: &'static str = "AsyncFieldList";
    const PACKAGE_NAME: &'static str = "Rzarrs";

    fn length(&mut self) -> usize {
        self.names.len()
    }

    fn elt(&mut self, i: usize) -> Sexp {
        if self.cache[i].is_none() {
            let task = self.tasks[i]
                .take()
                .expect("AsyncFieldList task already consumed");
            let value = match self.runtime.block_on(task) {
                Ok(Ok(value)) => value,
                Ok(Err(e)) => LazyValue::Utf8(vec![e]),
                Err(e) => LazyValue::Utf8(vec![e.to_string()]),
            };
            self.cache[i] = Some(value);
        }
        self.cache[i].as_ref().unwrap().to_sexp()
    }
}

/// Minimal internal smoke-test constructor. Replace the task body with chunked Zarr IO
/// when wiring a real exported async/ALTREP API.
fn async_i32_smoke() -> savvy::Result<Sexp> {
    let runtime = background_runtime()?;
    let task = runtime.spawn(async move { Ok(vec![1_i32, 2, 3]) });
    AsyncI32Vec {
        runtime,
        len: 3,
        task: Some(task),
        cache: None,
        error: None,
    }
    .into_altrep()
}

pub fn init_altrep_classes(dll_info: *mut DllInfo) -> savvy::Result<()> {
    register_altinteger_class::<AsyncI32Vec>(dll_info)?;
    register_altstring_class::<AsyncStringVec>(dll_info)?;
    register_altlist_class::<AsyncFieldList>(dll_info)?;
    Ok(())
}
