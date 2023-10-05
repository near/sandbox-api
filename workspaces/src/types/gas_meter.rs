use std::sync::{Arc, Mutex};

use super::Gas;
use crate::result::Result;
use crate::Worker;

/// A hook that is called on every transaction that is sent to the network.
/// This is useful for debugging purposes, or for tracking the amount of gas
/// that is being used.
pub type GasHook = Arc<dyn Fn(Gas) -> Result<()> + Send + Sync>;

/// Allows you to meter the amount of gas consumed by transaction(s).
/// Note: This only works with transactions that resolve to [`crate::result::ExecutionFinalResult`]
/// Example
/// ```rust, ignore, no_run
/// let mut worker = near_workspaces::sandbox().await?;
/// let meter = GasMeter::now(&mut worker);
///
/// let wasm = std::fs::read(STATUS_MSG_WASM_FILEPATH)?;
/// let contract = worker.dev_deploy(&wasm).await?;
///
/// contract
///    .call("set_status")
///    .args_json(json!({
///        "message": "hello_world",
///    }))
///    .transact()
///    .await?;
///
/// println!("Total Gas consumed: {}", meter.elapsed()?);
/// ```
pub struct GasMeter {
    gas: Arc<Mutex<Gas>>,
}

impl GasMeter {
    /// Create a new gas meter with 0 gas consumed.
    pub fn now<T: ?Sized>(worker: &mut Worker<T>) -> Self {
        let meter = Self {
            gas: Arc::new(Mutex::new(Gas::from_gas(0))),
        };

        let gas_consumed = Arc::downgrade(&Arc::clone(&meter.gas));
        worker.tx_callbacks.push(Arc::new(move |gas: Gas| {
            // upgrades if meter is still alive, else noop.
            _ = gas_consumed.upgrade().map(|consumed| {
                let mut consumed = consumed.lock().expect("GasMeter lock poisoned");
                *consumed = Gas::from_gas(consumed.as_gas() + gas.as_gas());
            });

            Ok(())
        }));

        meter
    }

    /// Get the total amount of gas consumed.
    pub fn elapsed(&self) -> Result<Gas> {
        let meter = self.gas.lock()?;
        Ok(*meter)
    }

    /// Reset the gas consumed to 0.
    pub fn reset(&self) -> Result<()> {
        let mut meter = self.gas.lock()?;
        *meter = Gas::from_gas(0);
        Ok(())
    }
}
