use lecturescribe_core::{AppError, ErrorCategory};
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::Path;
use std::ptr;

const SQLITE_OK: c_int = 0;
const SQLITE_ROW: c_int = 100;
const SQLITE_DONE: c_int = 101;
const SQLITE_INTEGER: c_int = 1;
const SQLITE_FLOAT: c_int = 2;
const SQLITE_TEXT: c_int = 3;
const SQLITE_NULL: c_int = 5;
const SQLITE_OPEN_READWRITE: c_int = 0x0000_0002;
const SQLITE_OPEN_CREATE: c_int = 0x0000_0004;
const SQLITE_OPEN_FULLMUTEX: c_int = 0x0001_0000;

type SqliteDestructor = Option<unsafe extern "C" fn(*mut c_void)>;

#[repr(C)]
struct sqlite3 {
    _private: [u8; 0],
}

#[repr(C)]
struct sqlite3_stmt {
    _private: [u8; 0],
}

#[cfg_attr(target_os = "windows", link(name = "winsqlite3"))]
#[cfg_attr(not(target_os = "windows"), link(name = "sqlite3"))]
extern "C" {
    fn sqlite3_open_v2(
        filename: *const c_char,
        database: *mut *mut sqlite3,
        flags: c_int,
        vfs: *const c_char,
    ) -> c_int;
    fn sqlite3_close(database: *mut sqlite3) -> c_int;
    fn sqlite3_errmsg(database: *mut sqlite3) -> *const c_char;
    fn sqlite3_busy_timeout(database: *mut sqlite3, milliseconds: c_int) -> c_int;
    fn sqlite3_exec(
        database: *mut sqlite3,
        sql: *const c_char,
        callback: *mut c_void,
        context: *mut c_void,
        error_message: *mut *mut c_char,
    ) -> c_int;
    fn sqlite3_prepare_v2(
        database: *mut sqlite3,
        sql: *const c_char,
        bytes: c_int,
        statement: *mut *mut sqlite3_stmt,
        tail: *mut *const c_char,
    ) -> c_int;
    fn sqlite3_finalize(statement: *mut sqlite3_stmt) -> c_int;
    fn sqlite3_step(statement: *mut sqlite3_stmt) -> c_int;
    fn sqlite3_reset(statement: *mut sqlite3_stmt) -> c_int;
    fn sqlite3_clear_bindings(statement: *mut sqlite3_stmt) -> c_int;
    fn sqlite3_bind_null(statement: *mut sqlite3_stmt, index: c_int) -> c_int;
    fn sqlite3_bind_int64(statement: *mut sqlite3_stmt, index: c_int, value: i64) -> c_int;
    fn sqlite3_bind_double(statement: *mut sqlite3_stmt, index: c_int, value: f64) -> c_int;
    fn sqlite3_bind_text(
        statement: *mut sqlite3_stmt,
        index: c_int,
        value: *const c_char,
        bytes: c_int,
        destructor: SqliteDestructor,
    ) -> c_int;
    fn sqlite3_column_count(statement: *mut sqlite3_stmt) -> c_int;
    fn sqlite3_column_type(statement: *mut sqlite3_stmt, column: c_int) -> c_int;
    fn sqlite3_column_int64(statement: *mut sqlite3_stmt, column: c_int) -> i64;
    fn sqlite3_column_double(statement: *mut sqlite3_stmt, column: c_int) -> f64;
    fn sqlite3_column_text(statement: *mut sqlite3_stmt, column: c_int) -> *const u8;
    fn sqlite3_column_bytes(statement: *mut sqlite3_stmt, column: c_int) -> c_int;
    fn sqlite3_changes(database: *mut sqlite3) -> c_int;
}

#[derive(Debug, Clone)]
pub enum SqlValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
}

impl SqlValue {
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            _ => None,
        }
    }

    pub fn integer(&self) -> Option<i64> {
        match self {
            Self::Integer(value) => Some(*value),
            _ => None,
        }
    }
}

impl From<&str> for SqlValue {
    fn from(value: &str) -> Self {
        Self::Text(value.to_string())
    }
}

impl From<String> for SqlValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<i64> for SqlValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl From<f64> for SqlValue {
    fn from(value: f64) -> Self {
        Self::Real(value)
    }
}

pub struct Connection {
    raw: *mut sqlite3,
}

impl Connection {
    pub fn open(path: &Path) -> Result<Self, AppError> {
        let path = CString::new(path.to_string_lossy().as_bytes()).map_err(|error| {
            database_error(
                "database_path_invalid",
                "The database path is invalid.",
                error,
            )
        })?;
        let mut raw = ptr::null_mut();
        let code = unsafe {
            sqlite3_open_v2(
                path.as_ptr(),
                &mut raw,
                SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE | SQLITE_OPEN_FULLMUTEX,
                ptr::null(),
            )
        };
        if code != SQLITE_OK || raw.is_null() {
            let detail = if raw.is_null() {
                format!("SQLite open failed with code {code}")
            } else {
                unsafe { error_message(raw) }
            };
            if !raw.is_null() {
                unsafe {
                    sqlite3_close(raw);
                }
            }
            return Err(AppError::new(
                "database_open_failed",
                ErrorCategory::Database,
                "LectureScribe could not open its local job database.",
                detail,
            ));
        }
        let connection = Self { raw };
        let timeout = unsafe { sqlite3_busy_timeout(connection.raw, 5_000) };
        connection.check(timeout, "database_busy_timeout")?;
        Ok(connection)
    }

    pub fn execute_batch(&self, sql: &str) -> Result<(), AppError> {
        let sql = c_string(sql, "database_sql_invalid")?;
        let code = unsafe {
            sqlite3_exec(
                self.raw,
                sql.as_ptr(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        self.check(code, "database_batch_failed")
    }

    pub fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<usize, AppError> {
        let mut statement = self.prepare(sql)?;
        statement.bind(params)?;
        let code = unsafe { sqlite3_step(statement.raw) };
        if code != SQLITE_DONE {
            return Err(self.error("database_execute_failed"));
        }
        Ok(unsafe { sqlite3_changes(self.raw) as usize })
    }

    pub fn query(&self, sql: &str, params: &[SqlValue]) -> Result<Vec<Vec<SqlValue>>, AppError> {
        let mut statement = self.prepare(sql)?;
        statement.bind(params)?;
        let mut rows = Vec::new();
        loop {
            match unsafe { sqlite3_step(statement.raw) } {
                SQLITE_ROW => rows.push(statement.row()),
                SQLITE_DONE => break,
                _ => return Err(self.error("database_query_failed")),
            }
        }
        Ok(rows)
    }

    pub fn transaction<T>(
        &self,
        operation: impl FnOnce(&Connection) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        self.execute_batch("BEGIN IMMEDIATE")?;
        match operation(self) {
            Ok(value) => {
                self.execute_batch("COMMIT")?;
                Ok(value)
            }
            Err(error) => {
                let _ = self.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    fn prepare(&self, sql: &str) -> Result<Statement<'_>, AppError> {
        let sql = c_string(sql, "database_sql_invalid")?;
        let mut raw = ptr::null_mut();
        let code =
            unsafe { sqlite3_prepare_v2(self.raw, sql.as_ptr(), -1, &mut raw, ptr::null_mut()) };
        self.check(code, "database_prepare_failed")?;
        Ok(Statement {
            raw,
            connection: self,
        })
    }

    fn check(&self, code: c_int, code_name: &str) -> Result<(), AppError> {
        if code == SQLITE_OK {
            Ok(())
        } else {
            Err(self.error(code_name))
        }
    }

    fn error(&self, code: &str) -> AppError {
        AppError::new(
            code,
            ErrorCategory::Database,
            "LectureScribe could not update its local job database.",
            unsafe { error_message(self.raw) },
        )
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        unsafe {
            sqlite3_close(self.raw);
        }
    }
}

struct Statement<'a> {
    raw: *mut sqlite3_stmt,
    connection: &'a Connection,
}

impl Statement<'_> {
    fn bind(&mut self, params: &[SqlValue]) -> Result<(), AppError> {
        unsafe {
            sqlite3_reset(self.raw);
            sqlite3_clear_bindings(self.raw);
        }
        for (offset, value) in params.iter().enumerate() {
            let index = offset as c_int + 1;
            let code = match value {
                SqlValue::Null => unsafe { sqlite3_bind_null(self.raw, index) },
                SqlValue::Integer(value) => unsafe { sqlite3_bind_int64(self.raw, index, *value) },
                SqlValue::Real(value) => unsafe { sqlite3_bind_double(self.raw, index, *value) },
                SqlValue::Text(value) => {
                    let value = c_string(value, "database_value_invalid")?;
                    unsafe {
                        sqlite3_bind_text(
                            self.raw,
                            index,
                            value.as_ptr(),
                            value.as_bytes().len() as c_int,
                            transient_destructor(),
                        )
                    }
                }
            };
            self.connection.check(code, "database_bind_failed")?;
        }
        Ok(())
    }

    fn row(&self) -> Vec<SqlValue> {
        let count = unsafe { sqlite3_column_count(self.raw) };
        (0..count)
            .map(
                |column| match unsafe { sqlite3_column_type(self.raw, column) } {
                    SQLITE_INTEGER => {
                        SqlValue::Integer(unsafe { sqlite3_column_int64(self.raw, column) })
                    }
                    SQLITE_FLOAT => {
                        SqlValue::Real(unsafe { sqlite3_column_double(self.raw, column) })
                    }
                    SQLITE_TEXT => {
                        let pointer = unsafe { sqlite3_column_text(self.raw, column) };
                        let length =
                            unsafe { sqlite3_column_bytes(self.raw, column) }.max(0) as usize;
                        if pointer.is_null() {
                            SqlValue::Text(String::new())
                        } else {
                            let bytes = unsafe { std::slice::from_raw_parts(pointer, length) };
                            SqlValue::Text(String::from_utf8_lossy(bytes).into_owned())
                        }
                    }
                    SQLITE_NULL => SqlValue::Null,
                    _ => SqlValue::Null,
                },
            )
            .collect()
    }
}

impl Drop for Statement<'_> {
    fn drop(&mut self) {
        unsafe {
            sqlite3_finalize(self.raw);
        }
    }
}

unsafe fn error_message(database: *mut sqlite3) -> String {
    let value = sqlite3_errmsg(database);
    if value.is_null() {
        "Unknown SQLite error".to_string()
    } else {
        CStr::from_ptr(value).to_string_lossy().into_owned()
    }
}

fn c_string(value: &str, code: &str) -> Result<CString, AppError> {
    CString::new(value.as_bytes())
        .map_err(|error| database_error(code, "Database text is invalid.", error))
}

fn database_error(code: &str, message: &str, error: impl std::fmt::Display) -> AppError {
    AppError::new(code, ErrorCategory::Database, message, error.to_string())
}

fn transient_destructor() -> SqliteDestructor {
    unsafe { std::mem::transmute::<isize, SqliteDestructor>(-1) }
}
