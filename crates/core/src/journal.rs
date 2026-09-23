use devguard_contract::*;
use rusqlite::{
    params, Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior,
};
use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::Duration;

pub(crate) struct Journal {
    connection: Connection,
}

pub(crate) fn db_error(_: rusqlite::Error) -> Error {
    Error::new(
        ErrorCode::JournalInvalid,
        "journal operation failed; admission remains closed",
    )
}
pub(crate) fn encode<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value)
        .map_err(|_| Error::new(ErrorCode::JournalInvalid, "journal encoding failed"))
}
pub(crate) fn decode<T: serde::de::DeserializeOwned>(value: &str) -> Result<T> {
    serde_json::from_str(value)
        .map_err(|_| Error::new(ErrorCode::JournalInvalid, "invalid journal record"))
}

impl Journal {
    pub(crate) fn initialize(path: &Path) -> Result<()> {
        // CREATE_NEW distinguishes bootstrap from recovery and refuses symlink/existing files.
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(|_| {
                Error::new(
                    ErrorCode::JournalInvalid,
                    "journal initialization requires a new file in an existing private directory",
                )
            })?;
        let connection = Self::connection(path)?;
        connection.execute_batch("BEGIN IMMEDIATE;
            CREATE TABLE metadata (name TEXT PRIMARY KEY, value TEXT NOT NULL);
            INSERT INTO metadata VALUES ('schema', '1');
            CREATE TABLE instances (
                consumer TEXT NOT NULL, generation TEXT NOT NULL, instance TEXT NOT NULL,
                identity TEXT NOT NULL, state TEXT NOT NULL CHECK (state IN ('active','suspect','retired')),
                registration_policy TEXT NOT NULL,
                PRIMARY KEY (consumer, generation, instance));
            CREATE TABLE attempts (
                consumer TEXT NOT NULL, generation TEXT NOT NULL, attempt TEXT NOT NULL,
                charged INTEGER NOT NULL CHECK (charged IN (0,1)), record TEXT NOT NULL,
                launch_hash TEXT,
                PRIMARY KEY (consumer, generation, attempt));
            CREATE INDEX active_attempts ON attempts(charged);
            CREATE TABLE retired_generations (
                consumer TEXT NOT NULL, generation TEXT NOT NULL,
                PRIMARY KEY (consumer, generation));
            COMMIT;").map_err(db_error)?;
        Ok(())
    }

    fn connection(path: &Path) -> Result<Connection> {
        let meta = std::fs::symlink_metadata(path)
            .map_err(|_| Error::new(ErrorCode::JournalInvalid, "journal must already exist"))?;
        if !meta.file_type().is_file() {
            return Err(Error::new(
                ErrorCode::JournalInvalid,
                "journal must be a regular file",
            ));
        }
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(db_error)?;
        connection
            .busy_timeout(Duration::from_millis(ADMISSION_DEADLINE_MS))
            .map_err(db_error)?;
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(db_error)?;
        connection
            .pragma_update(None, "synchronous", "FULL")
            .map_err(db_error)?;
        Ok(connection)
    }

    pub(crate) fn open(path: &Path) -> Result<Self> {
        let connection = Self::connection(path)?;
        let schema: String = connection
            .query_row("SELECT value FROM metadata WHERE name='schema'", [], |r| {
                r.get(0)
            })
            .map_err(db_error)?;
        if schema != "1" {
            return Err(Error::new(
                ErrorCode::JournalInvalid,
                "unsupported journal schema",
            ));
        }
        let health: String = connection
            .query_row("PRAGMA quick_check", [], |r| r.get(0))
            .map_err(db_error)?;
        if health != "ok" {
            return Err(Error::new(
                ErrorCode::JournalInvalid,
                "journal integrity check failed",
            ));
        }
        Ok(Self { connection })
    }

    pub(crate) fn transaction<T>(
        &mut self,
        action: impl FnOnce(&Transaction<'_>) -> Result<T>,
    ) -> Result<T> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let value = action(&tx)?;
        tx.commit().map_err(db_error)?;
        Ok(value)
    }
}

pub(crate) fn load(tx: &Transaction<'_>, key: &AttemptKey) -> Result<Option<AttemptRecord>> {
    let row: Option<(String, bool)> = tx.query_row(
        "SELECT record, charged FROM attempts WHERE consumer=?1 AND generation=?2 AND attempt=?3",
        params![key.consumer_id, key.consumer_generation, key.attempt_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional().map_err(db_error)?;
    row.map(|(raw, charged)| {
        let record: AttemptRecord = decode(&raw)?;
        if record.key != *key || record.phase.charged() != charged {
            return Err(Error::new(
                ErrorCode::JournalInvalid,
                "journal identity or accounting mismatch",
            ));
        }
        validate_record(&record)?;
        Ok(record)
    })
    .transpose()
}

pub(crate) fn validate_record(record: &AttemptRecord) -> Result<()> {
    if (record.phase == AttemptPhase::Released) != record.release_reason.is_some() {
        return Err(Error::new(
            ErrorCode::JournalInvalid,
            "release reason does not match execution phase",
        ));
    }
    // No helper can have been created for an attempt that recorded a scope,
    // and scope termination needs the scope it observed.
    let consistent = match record.release_reason {
        Some(ReleaseReason::NoHelperCreated) => record.scope.is_none() && record.applied.is_none(),
        Some(ReleaseReason::ScopeTerminated) => record.scope.is_some(),
        _ => true,
    };
    if !consistent {
        return Err(Error::new(
            ErrorCode::JournalInvalid,
            "release reason does not match the recorded scope",
        ));
    }
    record
        .key
        .validate()
        .map_err(|_| Error::new(ErrorCode::JournalInvalid, "invalid attempt identity"))?;
    if record.phase.charged() && (record.reservation.is_none() || record.plan.is_none()) {
        return Err(Error::new(
            ErrorCode::JournalInvalid,
            "charged attempt is missing its reservation",
        ));
    }
    if let Some(reservation) = &record.reservation {
        reservation
            .quantities
            .validate_workload()
            .map_err(|_| Error::new(ErrorCode::JournalInvalid, "invalid stored reservation"))?;
    }
    if matches!(
        record.phase,
        AttemptPhase::ScopeBound | AttemptPhase::RunAuthorized
    ) && (record.scope.is_none() || record.applied.is_none())
    {
        return Err(Error::new(
            ErrorCode::JournalInvalid,
            "bound attempt is missing its application evidence",
        ));
    }
    if record.phase == AttemptPhase::Denied
        && (record.reservation.is_some() || record.denial.is_none())
    {
        return Err(Error::new(
            ErrorCode::JournalInvalid,
            "denied attempt contains an invalid reservation",
        ));
    }
    Ok(())
}

pub(crate) fn validate_index(tx: &Transaction<'_>) -> Result<()> {
    let mut statement = tx
        .prepare("SELECT consumer,generation,attempt,record,charged FROM attempts")
        .map_err(db_error)?;
    let mut rows = statement.query([]).map_err(db_error)?;
    while let Some(row) = rows.next().map_err(db_error)? {
        let record: AttemptRecord = decode(&row.get::<_, String>(3).map_err(db_error)?)?;
        let key = AttemptKey {
            consumer_id: row.get(0).map_err(db_error)?,
            consumer_generation: row.get(1).map_err(db_error)?,
            attempt_id: row.get(2).map_err(db_error)?,
        };
        if record.key != key || record.phase.charged() != row.get::<_, bool>(4).map_err(db_error)? {
            return Err(Error::new(
                ErrorCode::JournalInvalid,
                "journal accounting index is inconsistent",
            ));
        }
        validate_record(&record)?;
    }
    Ok(())
}

pub(crate) fn save(tx: &Transaction<'_>, record: &AttemptRecord) -> Result<()> {
    validate_record(record)?;
    tx.execute("INSERT INTO attempts(consumer,generation,attempt,charged,record) VALUES (?1,?2,?3,?4,?5)
        ON CONFLICT(consumer,generation,attempt) DO UPDATE SET charged=excluded.charged,record=excluded.record",
        params![record.key.consumer_id, record.key.consumer_generation, record.key.attempt_id, record.phase.charged(), encode(record)?])
        .map_err(db_error)?;
    Ok(())
}

pub(crate) fn active(tx: &Transaction<'_>) -> Result<Vec<AttemptRecord>> {
    let mut statement = tx
        .prepare("SELECT consumer,generation,attempt,record FROM attempts WHERE charged=1")
        .map_err(db_error)?;
    let mut rows = statement.query([]).map_err(db_error)?;
    let mut records = Vec::new();
    while let Some(row) = rows.next().map_err(db_error)? {
        let record: AttemptRecord = decode(&row.get::<_, String>(3).map_err(db_error)?)?;
        let key = AttemptKey {
            consumer_id: row.get(0).map_err(db_error)?,
            consumer_generation: row.get(1).map_err(db_error)?,
            attempt_id: row.get(2).map_err(db_error)?,
        };
        if record.key != key || !record.phase.charged() {
            return Err(Error::new(
                ErrorCode::JournalInvalid,
                "active journal record is inconsistent",
            ));
        }
        validate_record(&record)?;
        records.push(record);
    }
    Ok(records)
}

pub(crate) fn expire_prepared(tx: &Transaction<'_>, now: &ObservationTime) -> Result<()> {
    for mut record in active(tx)? {
        if record.phase != AttemptPhase::Prepared {
            continue;
        }
        let reservation = record.reservation.as_ref().expect("validated reservation");
        if reservation.prepared_at.boot_id != now.boot_id
            || now.monotonic_ms >= reservation.prepare_deadline_ms
        {
            record.phase = AttemptPhase::Expired;
            save(tx, &record)?;
        }
    }
    Ok(())
}

pub(crate) fn committed(tx: &Transaction<'_>) -> Result<Budget> {
    active(tx)?.iter().try_fold(Budget::ZERO, |sum, record| {
        sum.checked_add(
            record
                .reservation
                .as_ref()
                .expect("validated reservation")
                .quantities,
        )
    })
}
