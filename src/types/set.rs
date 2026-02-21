//! Python set type — CPython 3.11 exact ABI layout.
//!
//! CPython layout (200 bytes on 64-bit):
//!   PyObject_HEAD           (16 bytes)
//!   Py_ssize_t fill         (8 bytes: active + dummy entries)
//!   Py_ssize_t used         (8 bytes: active entries only)
//!   Py_ssize_t mask         (8 bytes: tablesize - 1)
//!   setentry *table         (8 bytes: pointer to hash table)
//!   Py_hash_t hash          (8 bytes: only for frozenset)
//!   Py_ssize_t finger       (8 bytes: for pop())
//!   setentry smalltable[8]  (128 bytes: inline for small sets)
//!   PyObject *weakreflist   (8 bytes)

use crate::object::pyobject::RawPyObject;
use crate::object::typeobj::{RawPyTypeObject, PY_TPFLAGS_DEFAULT, PY_TPFLAGS_HAVE_GC};
use std::os::raw::c_int;
use std::ptr;

// ─── Struct layouts ───

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SetEntry {
    pub key: *mut RawPyObject,
    pub hash: isize,
}

const SET_ENTRY_SIZE: usize = std::mem::size_of::<SetEntry>(); // 16
const _: () = assert!(SET_ENTRY_SIZE == 16);

const SMALLTABLE_SIZE: usize = 8;

#[repr(C)]
pub struct PySetObject {
    pub ob_base: RawPyObject,                      // 16
    pub fill: isize,                               // 8
    pub used: isize,                               // 8
    pub mask: isize,                               // 8
    pub table: *mut SetEntry,                      // 8
    pub hash: isize,                               // 8
    pub finger: isize,                             // 8
    pub smalltable: [SetEntry; SMALLTABLE_SIZE],   // 128
    pub weakreflist: *mut RawPyObject,             // 8
}

const _: () = assert!(std::mem::size_of::<PySetObject>() == 200);

// ─── Type object ───

static mut SET_TYPE: RawPyTypeObject = {
    let mut tp = RawPyTypeObject::zeroed();
    tp.tp_name = b"set\0".as_ptr() as *const _;
    tp.tp_basicsize = 200; // sizeof(PySetObject)
    tp.tp_itemsize = 0;
    tp
};

pub unsafe fn set_type() -> *mut RawPyTypeObject {
    &mut SET_TYPE
}

// ─── Hashing and equality (reuse dict's) ───

fn hash_string(s: &str) -> isize {
    let mut h: u64 = 14695981039346656037;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    let h = h as isize;
    if h == -1 { -2 } else { h }
}

unsafe fn hash_object(obj: *mut RawPyObject) -> isize {
    if (*obj).ob_type == crate::types::unicode::unicode_type() {
        hash_string(crate::types::unicode::unicode_value(obj))
    } else if (*obj).ob_type == crate::types::longobject::long_type()
           || (*obj).ob_type == crate::types::boolobject::bool_type() {
        let v = crate::types::longobject::long_as_i64(obj);
        if v == -1 { -2 } else { v as isize }
    } else {
        let h = obj as isize;
        if h == -1 { -2 } else { h }
    }
}

unsafe fn set_keys_equal(a: *mut RawPyObject, b: *mut RawPyObject) -> bool {
    if a == b { return true; }
    if a.is_null() || b.is_null() { return false; }
    if (*a).ob_type == crate::types::unicode::unicode_type()
        && (*b).ob_type == crate::types::unicode::unicode_type()
    {
        return crate::types::unicode::unicode_value(a) == crate::types::unicode::unicode_value(b);
    }
    if ((*a).ob_type == crate::types::longobject::long_type()
        || (*a).ob_type == crate::types::boolobject::bool_type())
       && ((*b).ob_type == crate::types::longobject::long_type()
           || (*b).ob_type == crate::types::boolobject::bool_type())
    {
        return crate::types::longobject::long_as_i64(a) == crate::types::longobject::long_as_i64(b);
    }
    false
}

/// Sentinel for dummy (deleted) entries.
/// Uses a fixed address that can never be a valid Python object.
static mut DUMMY_STRUCT: u8 = 0;
unsafe fn dummy_key() -> *mut RawPyObject {
    &mut DUMMY_STRUCT as *mut u8 as *mut RawPyObject
}

#[inline]
fn is_active(entry: &SetEntry) -> bool {
    !entry.key.is_null() && entry.key != unsafe { dummy_key() }
}

// ─── Internal helpers ───

/// Get the smalltable pointer for a set object.
#[inline]
unsafe fn smalltable_ptr(s: *mut PySetObject) -> *mut SetEntry {
    &mut (*s).smalltable[0] as *mut SetEntry
}

/// Initialize a new set (after allocation).
unsafe fn set_init(s: *mut PySetObject) {
    (*s).fill = 0;
    (*s).used = 0;
    (*s).mask = (SMALLTABLE_SIZE - 1) as isize; // 7
    (*s).table = smalltable_ptr(s);
    (*s).hash = -1;
    (*s).finger = 0;
    (*s).weakreflist = ptr::null_mut();
    for i in 0..SMALLTABLE_SIZE {
        (*s).smalltable[i] = SetEntry { key: ptr::null_mut(), hash: 0 };
    }
}

/// Look up a key in the set. Returns pointer to the entry slot.
unsafe fn set_lookup(s: *mut PySetObject, key: *mut RawPyObject, hash: isize) -> *mut SetEntry {
    let mask = (*s).mask as usize;
    let table = (*s).table;
    let mut i = (hash as usize) & mask;
    let mut first_dummy: *mut SetEntry = ptr::null_mut();

    loop {
        let entry = &mut *table.add(i);
        if entry.key.is_null() {
            // Empty slot: return first dummy if we found one, else this slot
            return if !first_dummy.is_null() { first_dummy } else { entry };
        }
        if entry.key == dummy_key() {
            if first_dummy.is_null() {
                first_dummy = entry;
            }
        } else if entry.hash == hash && set_keys_equal(entry.key, key) {
            return entry; // Found it
        }
        i = (i + 1) & mask;
    }
}

/// Resize the set table. `min_used` is the minimum capacity needed.
unsafe fn set_resize(s: *mut PySetObject, min_used: isize) {
    // Find smallest power of 2 >= min_used * 2 (to keep load < 2/3)
    let mut new_size: usize = SMALLTABLE_SIZE;
    while (new_size as isize) * 2 / 3 < min_used {
        new_size <<= 1;
        if new_size > (1 << 30) { std::process::abort(); }
    }

    let old_table = (*s).table;
    let old_mask = (*s).mask as usize;
    let is_smalltable = old_table == smalltable_ptr(s);

    // Collect active entries
    let mut active: Vec<SetEntry> = Vec::with_capacity((*s).used as usize);
    for i in 0..=old_mask {
        let entry = &*old_table.add(i);
        if is_active(entry) {
            active.push(*entry);
        }
    }

    // Allocate new table
    let new_table = if new_size == SMALLTABLE_SIZE {
        // Use inline smalltable
        let t = smalltable_ptr(s);
        for i in 0..SMALLTABLE_SIZE {
            *t.add(i) = SetEntry { key: ptr::null_mut(), hash: 0 };
        }
        t
    } else {
        let t = libc::calloc(new_size, SET_ENTRY_SIZE) as *mut SetEntry;
        if t.is_null() {
            eprintln!("Fatal: out of memory growing set");
            std::process::abort();
        }
        t
    };

    (*s).table = new_table;
    (*s).mask = (new_size - 1) as isize;
    (*s).fill = 0;
    (*s).used = 0;

    // Re-insert active entries
    let new_mask = (*s).mask as usize;
    for entry in &active {
        let mut i = (entry.hash as usize) & new_mask;
        loop {
            let slot = &mut *new_table.add(i);
            if slot.key.is_null() {
                *slot = *entry;
                (*s).fill += 1;
                (*s).used += 1;
                break;
            }
            i = (i + 1) & new_mask;
        }
    }

    // Free old table if it was heap-allocated
    if !is_smalltable && !old_table.is_null() {
        libc::free(old_table as *mut libc::c_void);
    }
}

// ─── Dealloc ───

unsafe extern "C" fn set_dealloc(obj: *mut RawPyObject) {
    let s = obj as *mut PySetObject;
    let table = (*s).table;
    let mask = (*s).mask as usize;
    for i in 0..=mask {
        let entry = &*table.add(i);
        if is_active(entry) {
            (*entry.key).decref();
        }
    }
    // Free table if heap-allocated
    if table != smalltable_ptr(s) && !table.is_null() {
        libc::free(table as *mut libc::c_void);
    }
    crate::object::gc::PyObject_GC_Del(obj as *mut libc::c_void);
}

// ─── C API ───

#[no_mangle]
pub unsafe extern "C" fn PySet_New(_iterable: *mut RawPyObject) -> *mut RawPyObject {
    let obj = crate::object::gc::_PyObject_GC_New(&mut SET_TYPE) as *mut PySetObject;
    set_init(obj);
    obj as *mut RawPyObject
}

#[no_mangle]
pub unsafe extern "C" fn PyFrozenSet_New(iterable: *mut RawPyObject) -> *mut RawPyObject {
    // TODO: separate frozenset type
    PySet_New(iterable)
}

#[no_mangle]
pub unsafe extern "C" fn PySet_Add(
    set: *mut RawPyObject,
    key: *mut RawPyObject,
) -> c_int {
    if set.is_null() || key.is_null() { return -1; }
    let s = set as *mut PySetObject;
    let hash = hash_object(key);

    // Check if we need to resize (fill > 2/3 of table)
    let table_size = ((*s).mask + 1) as isize;
    if (*s).fill * 3 >= table_size * 2 {
        set_resize(s, (*s).used + 1);
    }

    let entry = set_lookup(s, key, hash);
    if is_active(&*entry) {
        // Already present
        return 0;
    }

    let was_dummy = (*entry).key == dummy_key();
    (*key).incref();
    (*entry).key = key;
    (*entry).hash = hash;
    (*s).used += 1;
    if !was_dummy {
        (*s).fill += 1;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn PySet_Discard(
    set: *mut RawPyObject,
    key: *mut RawPyObject,
) -> c_int {
    if set.is_null() || key.is_null() { return -1; }
    let s = set as *mut PySetObject;
    let hash = hash_object(key);
    let entry = set_lookup(s, key, hash);
    if !is_active(&*entry) {
        return 0; // not found
    }
    (*(*entry).key).decref();
    (*entry).key = dummy_key();
    (*entry).hash = -1;
    (*s).used -= 1;
    1
}

#[no_mangle]
pub unsafe extern "C" fn PySet_Contains(
    set: *mut RawPyObject,
    key: *mut RawPyObject,
) -> c_int {
    if set.is_null() || key.is_null() { return -1; }
    let s = set as *mut PySetObject;
    let hash = hash_object(key);
    let entry = set_lookup(s, key, hash);
    if is_active(&*entry) { 1 } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn PySet_Size(set: *mut RawPyObject) -> isize {
    if set.is_null() { return -1; }
    let s = set as *mut PySetObject;
    (*s).used
}

#[no_mangle]
pub unsafe extern "C" fn PySet_Clear(set: *mut RawPyObject) -> c_int {
    if set.is_null() { return -1; }
    let s = set as *mut PySetObject;
    let table = (*s).table;
    let mask = (*s).mask as usize;
    for i in 0..=mask {
        let entry = &mut *table.add(i);
        if is_active(entry) {
            (*entry.key).decref();
        }
        entry.key = ptr::null_mut();
        entry.hash = 0;
    }
    // Free table if heap-allocated
    if table != smalltable_ptr(s) && !table.is_null() {
        libc::free(table as *mut libc::c_void);
    }
    set_init(s);
    0
}

#[no_mangle]
pub unsafe extern "C" fn PySet_Check(obj: *mut RawPyObject) -> c_int {
    if obj.is_null() { return 0; }
    if (*obj).ob_type == set_type() { 1 } else { 0 }
}

#[no_mangle]
pub static mut PySet_Type: *mut RawPyTypeObject = ptr::null_mut();

pub unsafe fn init_set_type() {
    SET_TYPE.tp_dealloc = Some(set_dealloc);
    SET_TYPE.tp_flags = PY_TPFLAGS_DEFAULT | PY_TPFLAGS_HAVE_GC;
    PySet_Type = set_type();
}
