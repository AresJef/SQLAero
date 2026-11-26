//! Constructors for library-defined scalar calls with fixed or optional operands.

macro_rules! define_fixed_call {
    ($class:ident, $name:literal, $sql_name:literal, ($($argument:ident),*), $description:literal) => {
        #[doc = $description]
        #[pyclass(extends = SqlFunction, name = $name, module = "sqlaero.sqlfunc")]
        struct $class;

        #[pymethods]
        impl $class {
            #[new]
            fn new<'py>(
                py: Python<'py>,
                $($argument: Bound<'py, PyAny>),*
            ) -> PyResult<PyClassInitializer<Self>> {
                let arguments: &[Bound<'py, PyAny>] = &[$($argument),*];
                let operands = PyTuple::new(py, arguments)?;
                let form = Form::Call(concat!($sql_name, "("));
                let expression = SqlFunction::new($name, Kind::Scalar, form, operands)?;
                Ok(PyClassInitializer::from(expression).add_subclass(Self))
            }
        }
    };
}

macro_rules! fixed_calls {
    ($( $class:ident, $name:literal, $arguments:tt, $description:literal
        $(=> [$($alias:ident, $alias_name:literal),+])?; )+) => {
        $(
            super::calls::define_fixed_call!($class, $name, $name, $arguments, $description);
            $($(
                super::calls::define_fixed_call!($alias, $alias_name, $name, $arguments, $description);
            )+)?
        )+

        fn register_fixed(module: &Bound<'_, PyModule>) -> PyResult<()> {
            $(
                module.add_class::<$class>()?;
                $($(module.add_class::<$alias>()?;)+)?
            )+
            Ok(())
        }
    };
}

macro_rules! define_optional_call {
    ($class:ident, $name:literal, $sql_name:literal, ($($required:ident),*), ($($optional:ident),+),
      $signature:literal, $description:literal) => {
        #[doc = $description]
        #[pyclass(extends = SqlFunction, name = $name, module = "sqlaero.sqlfunc")]
        struct $class;

        #[pymethods]
        impl $class {
            #[new]
            #[pyo3(signature = ($($required,)* $($optional=crate::config::Argument::default()),+), text_signature = $signature)]
            fn new<'py>(
                py: Python<'py>,
                $($required: Bound<'py, PyAny>,)*
                $($optional: crate::config::Argument<'py>),+
            ) -> PyResult<PyClassInitializer<Self>> {
                let mut arguments = smallvec::SmallVec::<[Bound<'py, PyAny>; 6]>::new();
                $(arguments.push($required);)*
                let mut omitted = false;
                for argument in [$($optional.0),+] {
                    match argument {
                        None => omitted = true,
                        Some(value) if !omitted => arguments.push(value),
                        Some(_) => return Err(crate::errors::builtin_format::<pyo3::exceptions::PyTypeError>(
                            py, format_args!("{} cannot follow an omitted optional argument", $name),
                        )),
                    }
                }
                let operands = PyTuple::new(py, arguments)?;
                let form = Form::Call(concat!($sql_name, "("));
                let expression = SqlFunction::new($name, Kind::Scalar, form, operands)?;
                Ok(PyClassInitializer::from(expression).add_subclass(Self))
            }
        }
    };
}

macro_rules! optional_calls {
    ($( $class:ident, $name:literal, $required:tt, $optional:tt,
        $signature:literal, $description:literal $(=> [$($alias:ident, $alias_name:literal),+])?; )+) => {
        $(
            super::calls::define_optional_call!(
                $class, $name, $name, $required, $optional, $signature, $description
            );
            $($(
                super::calls::define_optional_call!(
                    $alias, $alias_name, $name, $required, $optional, $signature, $description
                );
            )+)?
        )+

        fn register_optional(module: &Bound<'_, PyModule>) -> PyResult<()> {
            $(
                module.add_class::<$class>()?;
                $($(module.add_class::<$alias>()?;)+)?
            )+
            Ok(())
        }
    };
}

macro_rules! variadic_calls {
    ($( $class:ident, $name:literal, ($($required:ident),+), $rest:ident, $description:literal; )+) => {
        $(
            #[doc = $description]
            #[pyclass(extends = SqlFunction, name = $name, module = "sqlaero.sqlfunc")]
            struct $class;

            #[pymethods]
            impl $class {
                #[new]
                #[pyo3(signature = ($($required,)+ *$rest))]
                fn new<'py>(
                    py: Python<'py>,
                    $($required: Bound<'py, PyAny>,)+
                    $rest: &Bound<'py, PyTuple>,
                ) -> PyResult<PyClassInitializer<Self>> {
                    let operands = super::calls::prefix_arguments(py, [$($required),+], $rest)?;
                    let expression = SqlFunction::new(
                        $name, Kind::Scalar, Form::Call(concat!($name, "(")), operands,
                    )?;
                    Ok(PyClassInitializer::from(expression).add_subclass(Self))
                }
            }
        )+

        fn register_variadic(module: &Bound<'_, PyModule>) -> PyResult<()> {
            $(module.add_class::<$class>()?;)+
            Ok(())
        }
    };
}

use pyo3::prelude::*;
use pyo3::types::PyTuple;

pub(super) fn prefix_arguments<'py, const N: usize>(
    py: Python<'py>,
    prefix: [Bound<'py, PyAny>; N],
    tail: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyTuple>> {
    let mut arguments = smallvec::SmallVec::<[Bound<'py, PyAny>; 6]>::new();
    arguments
        .try_reserve_exact(N + tail.len())
        .map_err(|_| crate::errors::memory_error(py))?;
    arguments.extend(prefix);
    for (index, argument) in tail.iter().enumerate() {
        if index.is_multiple_of(16_384) {
            py.check_signals()?;
        }
        arguments.push(argument);
    }
    PyTuple::new(py, arguments)
}

pub(super) use define_fixed_call;
pub(super) use define_optional_call;
pub(super) use fixed_calls;
pub(super) use optional_calls;
pub(super) use variadic_calls;
