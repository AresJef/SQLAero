//! Constructors for library-defined scalar calls with fixed or optional operands.

macro_rules! fixed_calls {
    ($( $class:ident, $name:literal, ($($argument:ident),*), $description:literal; )+) => {
        $(
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
                    let form = Form::Call(concat!($name, "("));
                    let expression = SqlFunction::new($name, Kind::Scalar, form, operands)?;
                    Ok(PyClassInitializer::from(expression).add_subclass(Self))
                }
            }
        )+

        fn register_fixed(module: &Bound<'_, PyModule>) -> PyResult<()> {
            $(module.add_class::<$class>()?;)+
            Ok(())
        }
    };
}

macro_rules! optional_calls {
    ($( $class:ident, $name:literal, ($($required:ident),*), $last:ident,
         $signature:literal, $description:literal; )+) => {
        $(
            #[doc = $description]
            #[pyclass(extends = SqlFunction, name = $name, module = "sqlaero.sqlfunc")]
            struct $class;

            #[pymethods]
            impl $class {
                #[new]
                #[pyo3(signature = ($($required,)* $last=crate::config::Argument::default()), text_signature = $signature)]
                fn new<'py>(
                    py: Python<'py>,
                    $($required: Bound<'py, PyAny>,)*
                    $last: crate::config::Argument<'py>,
                ) -> PyResult<PyClassInitializer<Self>> {
                    let mut arguments = smallvec::SmallVec::<[Bound<'py, PyAny>; 4]>::new();
                    $(arguments.push($required);)*
                    arguments.extend($last.0);
                    let operands = PyTuple::new(py, arguments)?;
                    let form = Form::Call(concat!($name, "("));
                    let expression = SqlFunction::new($name, Kind::Scalar, form, operands)?;
                    Ok(PyClassInitializer::from(expression).add_subclass(Self))
                }
            }
        )+

        fn register_optional(module: &Bound<'_, PyModule>) -> PyResult<()> {
            $(module.add_class::<$class>()?;)+
            Ok(())
        }
    };
}

pub(super) use fixed_calls;
pub(super) use optional_calls;
