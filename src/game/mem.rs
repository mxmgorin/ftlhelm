//! Reads and writes over the game's objects through `Obj`, a non-null pointer into one of
//! them. Field offsets come from `offsets`, taken from the binary's DWARF; an `Obj` is only
//! ever built from a game address or a pointer field of another `Obj`, so the reads below
//! trust the game's layout.

use std::ffi::{c_void, CStr};
use std::ptr::NonNull;

use super::api::{Point, Ptr, Rect};
use super::offsets::{VECTOR_BEGIN, VECTOR_END};

const PTR_SIZE: usize = std::mem::size_of::<usize>();

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

    /// Entry `index` of a game table of `size`-byte entries; None for a negative index.
    /// # Safety
    /// A non-negative `index` must lie within the table at `base`.
    pub unsafe fn table_entry(base: usize, index: i32, size: usize) -> Option<Obj> {
        Obj::from_addr(base + usize::try_from(index).ok()? * size)
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

    /// Element count of the std::vector of pointers at this address.
    pub fn ptr_count(self) -> usize {
        self.vec_len(PTR_SIZE)
    }

    /// Element i of the std::vector of pointers at this address, None for null or past
    /// the end.
    pub fn vec_obj(self, i: usize) -> Option<Obj> {
        if i >= self.ptr_count() {
            return None;
        }
        self.obj(VECTOR_BEGIN)?.obj(i * PTR_SIZE)
    }

    /// The non-null elements of the std::vector of pointers at this address, with their
    /// indices.
    pub fn vec_objs(self) -> impl Iterator<Item = (usize, Obj)> {
        (0..self.ptr_count()).filter_map(move |i| Some((i, self.vec_obj(i)?)))
    }

    /// Element i of the std::vector of `T` at this address, None past the end.
    pub fn vec_get<T: Copy>(self, i: usize) -> Option<T> {
        if i >= self.vec_len(std::mem::size_of::<T>()) {
            return None;
        }
        Some(self.obj(VECTOR_BEGIN)?.get(i * std::mem::size_of::<T>()))
    }

    /// The elements of the std::vector of `T` at this address.
    pub fn vec_items<T: Copy>(self) -> impl Iterator<Item = T> {
        (0..self.vec_len(std::mem::size_of::<T>())).filter_map(move |i| self.vec_get(i))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::fake::Arena;
    use std::mem::size_of;

    const PTR: usize = size_of::<usize>();

    #[test]
    fn a_vector_of_pointers_keeps_null_elements_in_place() {
        let mut arena = Arena::default();
        let owner = arena.alloc(VECTOR_END + PTR);
        assert_eq!(owner.vec_len(PTR), 0);
        assert_eq!(owner.vec_obj(0), None);
        let (a, b) = (arena.alloc(PTR), arena.alloc(PTR));
        arena.ptr_vector(owner, VECTOR_BEGIN, &[Some(a), None, Some(b)]);
        assert_eq!(owner.vec_len(PTR), 3);
        assert_eq!([0, 1, 2, 3].map(|i| owner.vec_obj(i)), [Some(a), None, Some(b), None]);
        assert_eq!(owner.ptr_count(), 3);
        assert_eq!(owner.vec_objs().collect::<Vec<_>>(), [(0, a), (2, b)]);
    }

    #[test]
    fn a_table_entry_needs_a_non_negative_index() {
        let mut arena = Arena::default();
        let table = arena.alloc(3 * PTR);
        // SAFETY: an index within the table.
        assert_eq!(unsafe { Obj::table_entry(table.addr(), 2, PTR) }, Some(table.at(2 * PTR)));
        // SAFETY: a negative index reads nothing.
        assert_eq!(unsafe { Obj::table_entry(table.addr(), -1, PTR) }, None);
    }

    #[test]
    fn a_vector_of_values_reads_each_element() {
        let mut arena = Arena::default();
        let owner = arena.alloc(VECTOR_END + PTR);
        let rects = [Rect { x: 1, y: 2, w: 3, h: 4 }, Rect { x: 5, y: 6, w: 7, h: 8 }];
        arena.vector(owner, VECTOR_BEGIN, &rects);
        assert_eq!(owner.vec_len(size_of::<Rect>()), 2);
        assert_eq!([0, 1, 2].map(|i| owner.vec_get::<Rect>(i)), [Some(rects[0]), Some(rects[1]), None]);
        assert_eq!(owner.vec_items::<Rect>().collect::<Vec<_>>(), rects);
    }

    #[test]
    fn an_unset_vector_has_no_elements() {
        let mut arena = Arena::default();
        let owner = arena.alloc(VECTOR_END + PTR);
        assert_eq!((owner.vec_objs().count(), owner.vec_items::<Rect>().count()), (0, 0));
    }
}
