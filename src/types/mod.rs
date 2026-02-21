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
        // 1. Initialize base types FIRST (metaclass + object base)
        crate::object::typeobj::init_base_types();

        // 2. Initialize _Py_NoneStruct early (before any extension can access it)
        (*none::_Py_NoneStruct.get()).ob_type = none::none_type();
        (*none::_Py_NoneStruct.get()).ob_refcnt =
            std::sync::atomic::AtomicIsize::new(isize::MAX / 2);

        // 3. Init individual type objects
        longobject::init_long_type();
        floatobject::init_float_type();
        boolobject::init_bool_type();
        list::init_list_type();
        tuple::init_tuple_type();
        bytes::init_bytes_type();
        unicode::init_unicode_type();
        dict::init_dict_type();
        set::init_set_type();
        funcobject::init_cfunction_type();

        // 4. Wire metaclass chain: set ob_type=PyType_Type on all built-in types
        use crate::object::typeobj::{PyType_Type, PyBaseObject_Type};

        macro_rules! wire_type {
            ($ty:expr) => {
                (*$ty.get()).ob_base.ob_type = PyType_Type.get();
                if (*$ty.get()).tp_base.is_null() {
                    (*$ty.get()).tp_base = PyBaseObject_Type.get();
                }
            };
        }

        wire_type!(longobject::PyLong_Type);
        wire_type!(floatobject::PyFloat_Type);
        wire_type!(boolobject::PyBool_Type);
        // bool's tp_base is already set to long_type in init_bool_type
        wire_type!(list::PyList_Type);
        wire_type!(tuple::PyTuple_Type);
        wire_type!(bytes::PyBytes_Type);
        wire_type!(unicode::PyUnicode_Type);
        wire_type!(dict::PyDict_Type);
        wire_type!(set::PySet_Type);

        // 5. Initialize exception hierarchy
        crate::runtime::error::init_exceptions();
    }

    // Touch singletons to force lazy initialization
    let _ = none::PY_NONE.get();
    let _ = boolobject::PY_TRUE.get();
    let _ = boolobject::PY_FALSE.get();
}
