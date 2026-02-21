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
    // Type objects are static and self-initialize.
    // Ensure they're all touched so static initializers run.
    let _ = none::PY_NONE.get();
    let _ = boolobject::PY_TRUE.get();
    let _ = boolobject::PY_FALSE.get();
}
