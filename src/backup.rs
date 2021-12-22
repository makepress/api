use std::convert::TryFrom;

use makepress_lib::uuid::Uuid;
use sled::{Db, Transactional, Tree};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BackupState {
    NotFound,
    Pending,
    Running,
    Error(String),
    Finished,
}

#[derive(Debug, Clone)]
pub(crate) struct BackupManager {
    pending: Tree,
    running: Tree,
    errored: Tree,
    finished: Tree,
}

impl TryFrom<Db> for BackupManager {
    type Error = sled::Error;
    fn try_from(db: Db) -> Result<Self, Self::Error> {
        Ok(Self {
            pending: db.open_tree(b"pending")?,
            running: db.open_tree(b"running")?,
            errored: db.open_tree(b"errored")?,
            finished: db.open_tree(b"finished")?,
        })
    }
}

impl BackupManager {
    pub fn get_status(&self, id: Uuid) -> Result<BackupState, sled::Error> {
        Ok(if self.pending.get(id.as_bytes())?.is_some() {
            BackupState::Pending
        } else if self.running.get(id.as_bytes())?.is_some() {
            BackupState::Running
        } else if let Some(error) = self.errored.get(id.as_bytes())? {
            BackupState::Error(String::from_utf8(error.to_vec()).unwrap())
        } else if self.finished.get(id.as_bytes())?.is_some() {
            BackupState::Finished
        } else {
            BackupState::NotFound
        })
    }

    pub fn set_pending(&self, id: Uuid) -> Result<(), sled::transaction::TransactionError> {
        (&self.pending, &self.running, &self.errored, &self.finished).transaction(
            |(p, r, e, f)| {
                p.insert(id.as_bytes(), vec![])?;
                r.remove(id.as_bytes())?;
                e.remove(id.as_bytes())?;
                f.remove(id.as_bytes())?;
                Ok(())
            },
        )?;
        Ok(())
    }

    pub fn set_running(&self, id: Uuid) -> Result<(), sled::transaction::TransactionError> {
        (&self.pending, &self.running, &self.errored, &self.finished).transaction(
            |(p, r, e, f)| {
                r.insert(id.as_bytes(), vec![])?;
                p.remove(id.as_bytes())?;
                e.remove(id.as_bytes())?;
                f.remove(id.as_bytes())?;
                Ok(())
            },
        )?;
        Ok(())
    }

    pub fn set_errored(
        &self,
        id: Uuid,
        error: String,
    ) -> Result<(), sled::transaction::TransactionError> {
        (&self.pending, &self.running, &self.errored, &self.finished).transaction(
            |(p, r, e, f)| {
                e.insert(id.as_bytes(), error.as_bytes())?;
                p.remove(id.as_bytes())?;
                r.remove(id.as_bytes())?;
                f.remove(id.as_bytes())?;
                Ok(())
            },
        )?;
        Ok(())
    }

    pub fn set_finished(&self, id: Uuid) -> Result<(), sled::transaction::TransactionError> {
        (&self.pending, &self.running, &self.errored, &self.finished).transaction(
            |(p, r, e, f)| {
                f.insert(id.as_bytes(), vec![])?;
                p.remove(id.as_bytes())?;
                r.remove(id.as_bytes())?;
                e.remove(id.as_bytes())?;
                Ok(())
            },
        )?;
        Ok(())
    }

    pub fn set_notfound(&self, id: Uuid) -> Result<(), sled::transaction::TransactionError> {
        (&self.pending, &self.running, &self.errored, &self.finished).transaction(
            |(p, r, e, f)| {
                p.remove(id.as_bytes())?;
                r.remove(id.as_bytes())?;
                e.remove(id.as_bytes())?;
                f.remove(id.as_bytes())?;
                Ok(())
            },
        )?;
        Ok(())
    }

    pub fn set_status(
        &self,
        id: Uuid,
        status: BackupState,
    ) -> Result<(), sled::transaction::TransactionError> {
        match status {
            BackupState::NotFound => self.set_notfound(id),
            BackupState::Pending => self.set_pending(id),
            BackupState::Running => self.set_running(id),
            BackupState::Error(error) => self.set_errored(id, error),
            BackupState::Finished => self.set_finished(id),
        }
    }
}
