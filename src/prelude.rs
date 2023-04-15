pub use crate::conf::{setting, Conf};
pub use crate::models;
pub use anyhow::{anyhow, Error, Result};
pub use log::{error, info, warn};
pub use rusqlite::Connection as DbConn;

use std::sync::{Arc, Mutex};

pub type DbClient = Arc<Mutex<DbConn>>;

#[cfg(test)]
pub fn db_client(db: DbConn) -> DbClient {
    Arc::new(Mutex::new(db))
}
