//! Built-in Python types: int, float, str, bool, None, list, dict, tuple, set, bytes.

pub mod none;
pub mod boolobject;
pub mod longobject;
pub mod floatobject;
pub mod unicode;
pub mod bytes;
pub mod list;
pub mod tuple;
pub mod dict;
pub mod set;
pub mod moduleobject;
pub mod funcobject;

/// Initialize all built-in type objects.
/// Must be called once at startup before any objects are created.
pub fn init_types() {
    unsafe {
        // Init long type first (bool depends on it)
        longobject::init_long_type();
        floatobject::init_float_type();
        boolobject::init_bool_type();
        list::init_list_type();
        tuple::init_tuple_type();
        bytes::init_bytes_type();
        unicode::init_unicode_type();
        dict::init_dict_type();
        set::init_set_type();
    }
    // Touch singletons to force lazy initialization
    let _ = none::PY_NONE.get();
    let _ = boolobject::PY_TRUE.get();
    let _ = boolobject::PY_FALSE.get();
}
