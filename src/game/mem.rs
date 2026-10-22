//! Reads and writes over the game's objects through `Obj`, a non-null pointer into one of
//! them. Field offsets come from `offsets`, taken from the binary's DWARF; an `Obj` is only
//! ever built from a game address or a pointer field of another `Obj`, so the reads below
//! trust the game's layout.

use std::ffi::{c_void, CStr};
use std::ptr::NonNull;

use super::api::{Point, Ptr, Rect};
use super::offsets::{VECTOR_BEGIN, VECTOR_END};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Obj(NonNull<u8>);

impl Obj {
    /// Wraps a pointer to a game object; None for null.
    /// # Safety
    /// A non-null `p` must point at a live object of the game, laid out as `offsets` says.
    pub unsafe fn from_ptr(p: *const c_void) -> Option<Obj> {
        NonNull::new(p as *mut u8).map(Obj)
    }

    /// Wraps the game object at a resolved symbol address.
    /// # Safety: as for `from_ptr`.
    pub unsafe fn from_addr(addr: usize) -> Option<Obj> {
        Obj::from_ptr(addr as *const c_void)
    }

    pub fn ptr(self) -> Ptr {
        self.0.as_ptr() as Ptr
    }

    pub fn addr(self) -> usize {
        self.0.as_ptr() as usize
    }

    /// The embedded object at a field offset.
    pub fn at(self, off: usize) -> Obj {
        // SAFETY: an embedded field of a live object stays inside it and is non-null.
        Obj(unsafe { NonNull::new_unchecked(self.0.as_ptr().add(off)) })
    }

    pub fn get<T: Copy>(self, off: usize) -> T {
        // SAFETY: the field at `off` holds a `T`, per the type's layout.
        unsafe { (self.0.as_ptr().add(off) as *const T).read_unaligned() }
    }

    pub fn set<T: Copy>(self, off: usize, v: T) {
        // SAFETY: as for `get`; the game owns the memory and runs on this thread.
        unsafe { (self.0.as_ptr().add(off) as *mut T).write_unaligned(v) }
    }

    /// The object a pointer field points at, or None for null.
    pub fn obj(self, off: usize) -> Option<Obj> {
        // SAFETY: pointer fields of game objects point at game objects or are null.
        unsafe { Obj::from_ptr(self.get::<*const c_void>(off)) }
    }

    pub fn i32(self, off: usize) -> i32 {
        self.get(off)
    }

    pub fn f32(self, off: usize) -> f32 {
        self.get(off)
    }

    /// A C++ bool field.
    pub fn flag(self, off: usize) -> bool {
        self.get::<u8>(off) != 0
    }

    pub fn point(self, off: usize) -> Point {
        self.get(off)
    }

    pub fn rect(self, off: usize) -> Rect {
        self.get(off)
    }

    /// Element count of the std::vector at this address.
    pub fn vec_len(self, elem: usize) -> usize {
        let b = self.get::<usize>(VECTOR_BEGIN);
        let e = self.get::<usize>(VECTOR_END);
        if b != 0 && e >= b {
            (e - b) / elem
        } else {
            0
        }
    }

    /// Element i of the std::vector of pointers at this address, None for null or past
    /// the end.
    pub fn vec_obj(self, i: usize) -> Option<Obj> {
        if i >= self.vec_len(std::mem::size_of::<usize>()) {
            return None;
        }
        self.obj(VECTOR_BEGIN)?.obj(i * std::mem::size_of::<usize>())
    }

    /// Element i of the std::vector of `T` at this address; the caller keeps i in range.
    pub fn vec_get<T: Copy>(self, i: usize) -> Option<T> {
        if i >= self.vec_len(std::mem::size_of::<T>()) {
            return None;
        }
        Some(self.obj(VECTOR_BEGIN)?.get(i * std::mem::size_of::<T>()))
    }

    /// Whether `p` lies within `size` bytes from this object.
    pub fn contains(self, size: usize, p: usize) -> bool {
        p >= self.addr() && p < self.addr() + size
    }

    /// The text of the game's std::string at a field offset: its data pointer, read up to
    /// the terminator.
    pub fn string(self, off: usize) -> String {
        match self.obj(off) {
            // SAFETY: a std::string's data is NUL-terminated.
            Some(chars) => unsafe { CStr::from_ptr(chars.ptr() as *const _) }.to_string_lossy().into_owned(),
            None => String::new(),
        }
    }
}
