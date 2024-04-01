use std::io::ErrorKind;
use std::thread;
use std::time::Duration;

use pyo3::exceptions::{ PyException, PyFileNotFoundError, PyRuntimeError, PyValueError };
use pyo3::prelude::*;
use pyo3::types::{ PyDict, PyType };

use crate::def::{ DirEntry, DirEntryExt, ReturnType };
use scandir::{ ErrorsType, ScandirResult, ScandirResultsType };

fn result2py(result: &ScandirResult, py: Python) -> Option<PyObject> {
    match result {
        ScandirResult::DirEntry(e) => { Some(Py::new(py, DirEntry::new(e)).unwrap().to_object(py)) }
        ScandirResult::DirEntryExt(e) => {
            Some(Py::new(py, DirEntryExt::new(e)).unwrap().to_object(py))
        }
        ScandirResult::Error((_path, _e)) => None,
    }
}

#[pyclass]
#[derive(Debug)]
pub struct Scandir {
    instance: scandir::Scandir,
    entries: ScandirResultsType,
    errors: ErrorsType,
}

#[pymethods]
impl Scandir {
    #[new]
    pub fn new(
        root_path: &str,
        sorted: Option<bool>,
        skip_hidden: Option<bool>,
        max_depth: Option<usize>,
        max_file_cnt: Option<usize>,
        dir_include: Option<Vec<String>>,
        dir_exclude: Option<Vec<String>>,
        file_include: Option<Vec<String>>,
        file_exclude: Option<Vec<String>>,
        case_sensitive: Option<bool>,
        return_type: Option<ReturnType>,
        store: Option<bool>
    ) -> PyResult<Self> {
        let return_type = return_type.unwrap_or(ReturnType::Base).from_object();
        Ok(Scandir {
            instance: match scandir::Scandir::new(root_path, store) {
                Ok(s) =>
                    s
                        .sorted(sorted.unwrap_or(false))
                        .skip_hidden(skip_hidden.unwrap_or(false))
                        .max_depth(max_depth.unwrap_or(0))
                        .max_file_cnt(max_file_cnt.unwrap_or(0))
                        .dir_include(dir_include)
                        .dir_exclude(dir_exclude)
                        .file_include(file_include)
                        .file_exclude(file_exclude)
                        .case_sensitive(case_sensitive.unwrap_or(false))
                        .return_type(return_type),
                Err(e) =>
                    match e.kind() {
                        ErrorKind::NotFound => {
                            return Err(PyFileNotFoundError::new_err(e.to_string()));
                        }
                        _ => {
                            return Err(PyException::new_err(e.to_string()));
                        }
                    }
            },
            entries: Vec::new(),
            errors: Vec::new(),
        })
    }

    pub fn clear(&mut self) {
        self.instance.clear();
        self.entries.clear();
        self.errors.clear();
    }

    pub fn start(&mut self) -> PyResult<()> {
        self.instance.start().map_err(|e| PyException::new_err(e.to_string()))
    }

    pub fn join(&mut self, py: Python) -> PyResult<bool> {
        let result = py.allow_threads(|| self.instance.join());
        if !result {
            return Err(PyRuntimeError::new_err("Thread not running"));
        }
        Ok(true)
    }

    pub fn stop(&mut self) -> PyResult<bool> {
        if !self.instance.stop() {
            return Err(PyRuntimeError::new_err("Thread not running"));
        }
        Ok(true)
    }

    pub fn collect(&mut self, py: Python) -> PyResult<(Vec<PyObject>, ErrorsType)> {
        let (entries, errors) = py.allow_threads(|| self.instance.collect())?;
        let results = entries
            .iter()
            .filter_map(|r| result2py(r, py))
            .collect();
        Ok((results, errors))
    }

    pub fn has_results(&mut self, only_new: Option<bool>) -> bool {
        self.instance.has_results(only_new.unwrap_or(true))
    }

    pub fn results_cnt(&mut self, only_new: Option<bool>) -> usize {
        self.instance.results_cnt(only_new.unwrap_or(true))
    }

    pub fn results(&mut self, only_new: Option<bool>, py: Python) -> (Vec<PyObject>, ErrorsType) {
        let (entries, errors) = self.instance.results(only_new.unwrap_or(true));
        let results = entries
            .iter()
            .filter_map(|e| result2py(e, py))
            .collect();
        (results, errors)
    }

    pub fn has_entries(&mut self, only_new: Option<bool>) -> bool {
        self.instance.has_entries(only_new.unwrap_or(true))
    }

    pub fn entries_cnt(&mut self, only_new: Option<bool>) -> usize {
        self.instance.entries_cnt(only_new.unwrap_or(true))
    }

    pub fn entries(&mut self, only_new: Option<bool>, py: Python) -> Vec<PyObject> {
        self.instance
            .entries(only_new.unwrap_or(true))
            .iter()
            .filter_map(|e| result2py(e, py))
            .collect()
    }

    pub fn has_errors(&mut self) -> bool {
        self.instance.has_errors()
    }

    pub fn errors_cnt(&mut self) -> usize {
        self.instance.errors_cnt()
    }

    pub fn errors(&mut self, only_new: Option<bool>) -> ErrorsType {
        self.instance.errors(only_new.unwrap_or(true))
    }

    pub fn as_dict(&mut self, only_new: Option<bool>, py: Python) -> PyObject {
        let pyresults = PyDict::new_bound(py);
        let (entries, errors) = self.instance.results(only_new.unwrap_or(true));
        for entry in entries {
            let _ = match entry {
                ScandirResult::DirEntry(e) =>
                    pyresults.set_item(
                        e.path.clone().into_py(py),
                        Py::new(py, DirEntry::new(&e)).unwrap().to_object(py)
                    ),
                ScandirResult::DirEntryExt(e) =>
                    pyresults.set_item(
                        e.path.clone().into_py(py),
                        Py::new(py, DirEntryExt::new(&e)).unwrap().to_object(py)
                    ),
                ScandirResult::Error((path, e)) => {
                    pyresults.set_item(path.into_py(py), e.to_object(py))
                }
            };
        }
        for error in errors {
            let _ = pyresults.set_item(error.0.into_py(py), error.1.to_object(py));
        }
        pyresults.to_object(py)
    }

    pub fn duration(&mut self) -> f64 {
        self.instance.duration()
    }

    pub fn finished(&mut self) -> bool {
        self.instance.finished()
    }

    pub fn busy(&self) -> bool {
        self.instance.busy()
    }

    fn __enter__(mut slf: PyRefMut<Self>) -> PyResult<PyRefMut<Self>> {
        slf.instance.start().map_err(|e| PyException::new_err(e.to_string()))?;
        Ok(slf)
    }

    fn __exit__(
        &mut self,
        ty: Option<Bound<PyType>>,
        _value: Option<Bound<PyAny>>,
        _traceback: Option<Bound<PyAny>>
    ) -> PyResult<bool> {
        if !self.instance.stop() {
            return Ok(false);
        }
        self.instance.join();
        match ty {
            Some(ty) => Python::with_gil(|py| ty.eq(py.get_type_bound::<PyValueError>())),
            None => Ok(false),
        }
    }

    fn __iter__(mut slf: PyRefMut<Self>) -> PyResult<PyRefMut<Self>> {
        if slf.instance.busy() {
            return Err(PyRuntimeError::new_err("Busy"));
        }
        slf.instance.start()?;
        slf.entries.clear();
        slf.errors.clear();
        Ok(slf)
    }

    fn __next__(&mut self, py: Python) -> PyResult<Option<PyObject>> {
        loop {
            if let Some(entry) = self.entries.pop() {
                match entry {
                    ScandirResult::DirEntry(e) => {
                        return Ok(Some(Py::new(py, DirEntry::new(&e)).unwrap().to_object(py)));
                    }
                    ScandirResult::DirEntryExt(e) => {
                        return Ok(Some(Py::new(py, DirEntryExt::new(&e)).unwrap().to_object(py)));
                    }
                    ScandirResult::Error(error) => {
                        return Ok(Some(error.to_object(py)));
                    }
                }
            }
            if let Some(error) = self.errors.pop() {
                return Ok(Some(error.to_object(py)));
            }
            let (entries, errors) = self.instance.results(true);
            if entries.is_empty() && errors.is_empty() {
                if !self.instance.busy() {
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            } else {
                self.entries.extend_from_slice(&entries);
                self.errors.extend_from_slice(&errors);
            }
        }
        Ok(None)
    }

    fn __str__(&self) -> PyResult<String> {
        Ok(format!("{:?}", self))
    }
}
