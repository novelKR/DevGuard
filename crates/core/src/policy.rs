use devguard_contract::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsumerRole {
    Workload,
    ControlService,
}

#[derive(Debug, Clone)]
pub struct ConsumerDefinition {
    pub generation: String,
    pub uid: u32,
    pub role: ConsumerRole,
    pub credential_digest: String,
    pub max_instances: u32,
    /// Static reservation per configured control instance. Not dynamically returned.
    pub control_reservation: Budget,
}

#[derive(Debug, Clone)]
pub struct Policy {
    pub revision: String,
    pub effective_capacity: Budget,
    pub host_headroom: Budget,
    /// Daemon and aggregate CLI control pool, counted once.
    pub system_reservation: Budget,
    pub consumers: BTreeMap<String, ConsumerDefinition>,
}

impl Policy {
    pub fn validate(&self) -> Result<()> {
        validate_id(&self.revision)?;
        self.effective_capacity.validate_workload()?;
        for (id, consumer) in &self.consumers {
            validate_id(id)?;
            validate_id(&consumer.generation)?;
            validate_digest(&consumer.credential_digest)?;
            if consumer.max_instances == 0 {
                return Err(Error::new(
                    ErrorCode::InvalidRequest,
                    "consumer instance limit must be positive",
                ));
            }
            if consumer.role == ConsumerRole::Workload
                && consumer.control_reservation != Budget::ZERO
            {
                return Err(Error::new(
                    ErrorCode::InvalidRequest,
                    "workload cannot grant itself a control reservation",
                ));
            }
            if consumer.role == ConsumerRole::ControlService {
                consumer.control_reservation.validate_workload()?;
            }
        }
        self.static_reservations()?;
        Ok(())
    }

    pub fn static_reservations(&self) -> Result<Budget> {
        let mut total = self.host_headroom.checked_add(self.system_reservation)?;
        for consumer in self.consumers.values() {
            total = total.checked_add(
                consumer
                    .control_reservation
                    .checked_mul(consumer.max_instances)?,
            )?;
        }
        Ok(total)
    }

    pub fn work_capacity(&self) -> Result<Budget> {
        Ok(self
            .effective_capacity
            .remaining_after(self.static_reservations()?))
    }
}
