//! Module for atomic pointer which can be statically initialized to NULL.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::marker::PhantomData;
use std::mem::transmute;

/// Macro for creating an `AtomicPtr` set to null in a static.
#[macro_export]
macro_rules! atomic_ptr_null {
    () => { $crate::AtomicPtr {
        ptr: ::std::sync::atomic::ATOMIC_USIZE_INIT,
        typ: ::std::marker::PhantomData,
    } }
}

/// Atomic pointer which can be stored and initialized in a static.
///
/// The `std::sync::atomic::AtomicPtr` cannot be initialized in a static to NULL without the
/// unstable feature const_fn.
///
/// All operations use `Ordering::SeqCst` which should guarantee along with `swap`  that memory
/// is not accessed twice.
// Fields need to be public to be able to initialize this statically
pub struct AtomicPtr<T: Send> {
    /// NOTE: Do not set manually, either use `AtomicPtr::new`, `AtomicPtr::empty` or
    /// `atomic_ptr_null!`.
    pub ptr: AtomicUsize,
    /// NOTE: Do not set manually, either use `AtomicPtr::new` or `atomic_ptr_null!`.
    pub typ: PhantomData<T>,
}

impl<T: Send> AtomicPtr<T> {
    #[inline]
    pub fn new(t: Box<T>) -> AtomicPtr<T> {
        AtomicPtr {
            ptr: AtomicUsize::new(unsafe { transmute::<_, *mut T>(t) } as usize),
            typ: PhantomData,
        }
    }

    #[inline]
    pub fn empty() -> AtomicPtr<T> {
        AtomicPtr {
            ptr: AtomicUsize::new(0),
            typ: PhantomData,
        }
    }

    #[inline]
    pub fn take(&mut self) -> Option<Box<T>> {
        match self.ptr.swap(0, Ordering::SeqCst) {
            0 => None,
            n => Some(unsafe { transmute::<_, Box<T>>(n) }),
        }
    }

    #[inline]
    pub fn swap(&mut self, t: Box<T>) -> Option<Box<T>> {
        match self.ptr.swap(unsafe { transmute::<_, *mut T>(t) } as usize, Ordering::SeqCst) {
            0 => None,
            n => Some(unsafe { transmute::<_, Box<T>>(n) }),
        }
    }
}
