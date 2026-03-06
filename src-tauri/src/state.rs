use std::sync::{Arc, Mutex};

use crate::storage::Storage;

pub struct AppState {
    pub storage: Arc<Mutex<Storage>>,
}

impl AppState {
    pub fn load() -> Result<Self, String> {
        let storage = Storage::load()?;
        Ok(Self {
            storage: Arc::new(Mutex::new(storage)),
        })
    }
}
