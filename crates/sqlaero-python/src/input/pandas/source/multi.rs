//! One-dimensional MultiIndex values, acquired as current-row facts before
//! either target emits that row. No Python tuple or formatted row is built.

use super::{Column, SourceTypes, inherits, invalid, numpy_source};
use crate::input::cell::{Schema, Target};
use crate::input::control::{self, Checkpoint};
use crate::input::numpy::{NumpyArrayApi, NumpySource};
use crate::input::source::{Child, PreparedSource, Selected, Shape};
use crate::{errors, input};
use pyo3::exceptions::PyIndexError;
use pyo3::prelude::*;
use smallvec::SmallVec;
use sqlaero_transcode::value::ScalarRef;

pub(super) enum Leaf<'py> {
    Native(ScalarRef<'static>),
    Python(Bound<'py, PyAny>),
    /// A qualified retained native source needs no Python scalar acquisition.
    Deferred(usize),
}

struct Level<'py> {
    owner: Bound<'py, PyAny>,
    length: usize,
    codes: NumpySource<'py>,
    declared: Option<Schema>,
    column: Option<Box<Column<'py>>>,
}

pub(super) struct Multi<'py> {
    owner: Bound<'py, PyAny>,
    length: usize,
    levels: Vec<Level<'py>>,
    catalog: Py<SourceTypes>,
    numpy: NumpyArrayApi,
    current: Option<usize>,
    values: SmallVec<[Leaf<'py>; 8]>,
    positions: SmallVec<[Option<usize>; 8]>,
}

pub(super) fn prepare<'py>(
    catalog: &Py<SourceTypes>,
    owner: Bound<'py, PyAny>,
    receiver: Bound<'py, PyAny>,
    length: usize,
    numpy: NumpyArrayApi,
    budget: &dyn Checkpoint,
) -> PyResult<Multi<'py>> {
    let py = owner.py();
    let registry = catalog.bind(py).get();
    let raw_levels =
        control::callback(py, Some(budget), || errors::attribute(&receiver, c"levels"))?;
    let raw_codes = control::callback(py, Some(budget), || errors::attribute(&receiver, c"codes"))?;
    let count = control::callback(py, Some(budget), || raw_levels.len())?;
    if control::callback(py, Some(budget), || raw_codes.len())? != count {
        return Err(invalid(
            py,
            "Pandas MultiIndex codes do not match its levels",
        ));
    }
    let mut levels = Vec::new();
    levels
        .try_reserve_exact(count)
        .map_err(|_| errors::memory_error(py))?;
    for index in 0..count {
        input::check(py, budget)?;
        let key = input::python_index(py, index)?;
        let level = control::callback(py, Some(budget), || raw_levels.get_item(&key))?;
        let base = registry
            .native_indices
            .iter()
            .rev()
            .find(|base| inherits(&level, base.bind(py)))
            .ok_or_else(|| {
                invalid(
                    py,
                    "MultiIndex level requires a supported actual Index base",
                )
            })?;
        let level = registry.receiver(&level, base.bind(py), budget)?;
        let dtype = control::callback(py, Some(budget), || errors::attribute(&level, c"dtype"))?;
        let declared = control::callback(py, Some(budget), || {
            registry.declared_schema(&dtype, &numpy, budget)
        })?;
        let labels = control::callback(py, Some(budget), || level.len())?;
        let codes = control::callback(py, Some(budget), || raw_codes.get_item(&key))?;
        let codes = numpy_source(&numpy, &codes, budget, length)?;
        levels.push(Level {
            owner: level,
            length: labels,
            codes,
            declared,
            column: None,
        });
    }
    Ok(Multi {
        owner,
        length,
        levels,
        catalog: catalog.clone_ref(py),
        numpy,
        current: None,
        values: SmallVec::new(),
        positions: SmallVec::new(),
    })
}

impl<'py> Multi<'py> {
    fn row(&mut self, row: usize, budget: &dyn Checkpoint) -> PyResult<()> {
        let py = self.owner.py();
        if row >= self.length {
            return Err(errors::builtin_message::<PyIndexError>(
                py,
                "Pandas row is outside its prepared shape",
            ));
        }
        if self.current == Some(row) {
            return Ok(());
        }
        self.current = None;
        self.values.clear();
        self.positions.clear();
        // Old owned Python leaves may run finalizers when released. Recheck
        // the original authority before any later source acquisition.
        input::check(py, budget)?;
        self.values
            .try_reserve(self.levels.len())
            .map_err(|_| errors::memory_error(py))?;
        self.positions
            .try_reserve(self.levels.len())
            .map_err(|_| errors::memory_error(py))?;
        for level in &mut self.levels {
            let code = level.codes.signed_at(row, budget)?;
            let position = if code == -1 {
                None
            } else {
                // Public verify_integrity=False can preserve other negative
                // codes; their actual Index indexing is relative to the end.
                let index = if code < 0 {
                    usize::try_from(code.unsigned_abs())
                        .ok()
                        .and_then(|n| level.length.checked_sub(n))
                } else {
                    usize::try_from(code).ok()
                };
                Some(index.filter(|n| *n < level.length).ok_or_else(|| {
                    errors::builtin_format::<PyIndexError>(
                        py,
                        format_args!(
                            "MultiIndex code {code} is outside its {} labels",
                            level.length
                        ),
                    )
                })?)
            };
            self.positions.push(position);
        }
        for (level, position) in self.levels.iter_mut().zip(&self.positions) {
            input::check(py, budget)?;
            let Some(position) = *position else {
                self.values.push(Leaf::Native(ScalarRef::Null));
                continue;
            };
            if level.column.is_none() {
                let mut column = self.catalog.bind(py).get().index_column(
                    &level.owner,
                    &self.numpy,
                    budget,
                    &self.catalog,
                )?;
                if column.length()? != level.length {
                    return Err(invalid(
                        py,
                        "MultiIndex label storage changed length during acquisition",
                    ));
                }
                if let Column::Arrow(source) = &mut column {
                    source.enable_random(budget)?;
                }
                level.column = Some(input::boxed(py, column)?);
            }
            let source = level
                .column
                .as_mut()
                .ok_or_else(|| invalid(py, "MultiIndex label source was not prepared"))?;
            self.values.push(source.capture(position, budget)?);
        }
        input::check(py, budget)?;
        self.current = Some(row);
        Ok(())
    }

    fn leaf(
        &mut self,
        index: usize,
        target: &mut dyn Target,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        consume_leaf(
            &self.owner,
            &self.values,
            &mut self.levels,
            &self.numpy,
            index,
            target,
            budget,
        )
    }

    crate::input::cell::destinations!();
    fn consume_cell_to(
        &mut self,
        row: usize,
        column: usize,
        target: &mut dyn Target,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        self.consume_cell(row, column, target, budget)
    }
}

impl<'py> PreparedSource<'py> for Multi<'py> {
    fn owner(&self) -> &Bound<'py, PyAny> {
        &self.owner
    }
    fn shape(&self) -> Shape {
        Shape::Vector {
            length: self.length,
        }
    }
    fn schema(&self, target: &mut dyn Target) -> PyResult<()> {
        for level in &self.levels {
            target.checkpoint(self.owner.py(), 1)?;
            if let Some(schema) = level.declared {
                target.schema(self.owner.py(), schema)?;
            }
            if let Some(column) = &level.column {
                column.schema(target)?;
            }
        }
        Ok(())
    }
    fn child<'a>(&'a mut self, index: usize, budget: &dyn Checkpoint) -> PyResult<Child<'a, 'py>> {
        self.row(index, budget)?;
        let shape = Shape::Vector {
            length: self.levels.len(),
        };
        Ok(Child::Native(Selected::new(self, index, shape)))
    }
    fn consume_child(
        &mut self,
        index: usize,
        row: usize,
        column: usize,
        target: &mut dyn Target,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        self.row(index, budget)?;
        if column != 0 {
            return Err(invalid(
                self.owner.py(),
                "MultiIndex label row has one positional axis",
            ));
        }
        self.leaf(row, target, budget)
    }
    fn consume_cell(
        &mut self,
        row: usize,
        column: usize,
        target: &mut dyn Target,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        if column != 0 {
            return Err(invalid(
                self.owner.py(),
                "MultiIndex is a one-dimensional value series",
            ));
        }
        self.row(row, budget)?;
        target.source(
            &mut Row {
                owner: &self.owner,
                values: &self.values,
                levels: &mut self.levels,
                numpy: &self.numpy,
            },
            budget,
        )
    }
    crate::input::source::cell_destinations!();
}

fn consume_leaf(
    owner: &Bound<'_, PyAny>,
    values: &[Leaf<'_>],
    levels: &mut [Level<'_>],
    numpy: &NumpyArrayApi,
    index: usize,
    target: &mut dyn Target,
    budget: &dyn Checkpoint,
) -> PyResult<()> {
    target.checkpoint(owner.py(), 1)?;
    match values
        .get(index)
        .ok_or_else(|| invalid(owner.py(), "MultiIndex label is outside its current row"))?
    {
        Leaf::Native(value) => target.scalar(owner.py(), *value),
        Leaf::Python(value) => target.python(value, None),
        Leaf::Deferred(position) => levels
            .get_mut(index)
            .and_then(|level| level.column.as_mut())
            .ok_or_else(|| {
                invalid(
                    owner.py(),
                    "MultiIndex deferred label has no retained source",
                )
            })?
            .consume(*position, target, budget, numpy),
    }
}

struct Row<'a, 'py> {
    owner: &'a Bound<'py, PyAny>,
    values: &'a [Leaf<'py>],
    levels: &'a mut [Level<'py>],
    numpy: &'a NumpyArrayApi,
}
impl<'py> PreparedSource<'py> for Row<'_, 'py> {
    fn owner(&self) -> &Bound<'py, PyAny> {
        self.owner
    }
    fn shape(&self) -> Shape {
        Shape::Vector {
            length: self.values.len(),
        }
    }
    fn schema(&self, _target: &mut dyn Target) -> PyResult<()> {
        Ok(())
    }
    fn consume_cell(
        &mut self,
        row: usize,
        column: usize,
        target: &mut dyn Target,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        if column != 0 {
            return Err(invalid(
                self.owner.py(),
                "MultiIndex current row has one positional axis",
            ));
        }
        consume_leaf(
            self.owner,
            self.values,
            self.levels,
            self.numpy,
            row,
            target,
            budget,
        )
    }
    crate::input::source::cell_destinations!();
}
impl Row<'_, '_> {
    crate::input::cell::destinations!();
    fn consume_cell_to(
        &mut self,
        row: usize,
        column: usize,
        target: &mut dyn Target,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        self.consume_cell(row, column, target, budget)
    }
}
