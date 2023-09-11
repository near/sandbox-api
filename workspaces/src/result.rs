//! Result and execution types from results of RPC calls to the network.

use std::fmt;

use near_account_id::AccountId;
use near_primitives::errors::TxExecutionError;
use near_primitives::views::{
    CallResult, ExecutionOutcomeWithIdView, ExecutionStatusView, FinalExecutionOutcomeView,
    FinalExecutionStatus,
};

use crate::error::ErrorKind;
use crate::types::{Balance, CryptoHash, Gas};

use base64::{engine::general_purpose, Engine as _};

pub type Result<T, E = crate::error::Error> = core::result::Result<T, E>;

/// Execution related info as a result of performing a successful transaction
/// execution on the network. This value can be converted into the returned
/// value of the transaction via [`ExecutionSuccess::json`] or [`ExecutionSuccess::borsh`]
pub type ExecutionSuccess = ExecutionResult<Value>;

/// Execution related info as a result of performing a failed transaction
/// execution on the network. The related error message can be retrieved
/// from this object or can be forwarded.
pub type ExecutionFailure = ExecutionResult<TxExecutionError>;

/// Struct to hold a type we want to return along w/ the execution result view.
/// This view has extra info about the execution, such as gas usage and whether
/// the transaction failed to be processed on the chain.
#[non_exhaustive]
#[must_use = "use `into_result()` to handle potential execution errors"]
pub struct Execution<T> {
    pub result: T,
    pub details: ExecutionFinalResult,
}

impl<T> Execution<T> {
    pub fn unwrap(self) -> T {
        self.into_result().unwrap()
    }

    #[allow(clippy::result_large_err)]
    pub fn into_result(self) -> Result<T, ExecutionFailure> {
        self.details.into_result()?;
        Ok(self.result)
    }

    /// Checks whether the transaction was successful. Returns true if
    /// the transaction has a status of FinalExecutionStatus::Success.
    pub fn is_success(&self) -> bool {
        self.details.is_success()
    }

    /// Checks whether the transaction has failed. Returns true if
    /// the transaction has a status of FinalExecutionStatus::Failure.
    pub fn is_failure(&self) -> bool {
        self.details.is_failure()
    }
}

/// The transaction/receipt details of a transaction execution. This object
/// can be used to retrieve data such as logs and gas burnt per transaction
/// or receipt.
#[derive(PartialEq, Eq, Clone)]
pub(crate) struct ExecutionDetails {
    pub(crate) transaction: ExecutionOutcome,
    pub(crate) receipts: Vec<ExecutionOutcome>,
}

impl ExecutionDetails {
    /// Returns just the transaction outcome.
    pub fn outcome(&self) -> &ExecutionOutcome {
        &self.transaction
    }

    /// Grab all outcomes after the execution of the transaction. This includes outcomes
    /// from the transaction and all the receipts it generated.
    pub fn outcomes(&self) -> Vec<&ExecutionOutcome> {
        let mut outcomes = vec![&self.transaction];
        outcomes.extend(self.receipt_outcomes());
        outcomes
    }

    /// Grab all outcomes after the execution of the transaction. This includes outcomes
    /// only from receipts generated by this transaction.
    pub fn receipt_outcomes(&self) -> &[ExecutionOutcome] {
        &self.receipts
    }

    /// Grab all outcomes that did not succeed the execution of this transaction. This
    /// will also include the failures from receipts as well.
    pub fn failures(&self) -> Vec<&ExecutionOutcome> {
        let mut failures = Vec::new();
        if matches!(self.transaction.status, ExecutionStatusView::Failure(_)) {
            failures.push(&self.transaction);
        }
        failures.extend(self.receipt_failures());
        failures
    }

    /// Just like `failures`, grab only failed receipt outcomes.
    pub fn receipt_failures(&self) -> Vec<&ExecutionOutcome> {
        self.receipts
            .iter()
            .filter(|receipt| matches!(receipt.status, ExecutionStatusView::Failure(_)))
            .collect()
    }

    /// Grab all logs from both the transaction and receipt outcomes.
    pub fn logs(&self) -> Vec<&str> {
        self.outcomes()
            .iter()
            .flat_map(|outcome| &outcome.logs)
            .map(String::as_str)
            .collect()
    }
}

/// The result after evaluating the status of an execution. This can be [`ExecutionSuccess`]
/// for successful executions or a [`ExecutionFailure`] for failed ones.
#[derive(PartialEq, Eq, Clone)]
#[non_exhaustive]
pub struct ExecutionResult<T> {
    /// Total gas burnt by the execution
    pub total_gas_burnt: Gas,

    /// Value returned from an execution. This is a base64 encoded str for a successful
    /// execution or a `TxExecutionError` if a failed one.
    pub(crate) value: T,
    // pub(crate) transaction: ExecutionOutcome,
    // pub(crate) receipts: Vec<ExecutionOutcome>,
    pub(crate) details: ExecutionDetails,
}

impl<T: fmt::Debug> fmt::Debug for ExecutionResult<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExecutionResult")
            .field("total_gas_burnt", &self.total_gas_burnt)
            .field("transaction", &self.details.transaction)
            .field("receipts", &self.details.receipts)
            .field("value", &self.value)
            .finish()
    }
}

/// Execution related info found after performing a transaction. Can be converted
/// into [`ExecutionSuccess`] or [`ExecutionFailure`] through [`into_result`]
///
/// [`into_result`]: crate::result::ExecutionFinalResult::into_result
#[derive(PartialEq, Eq, Clone)]
#[must_use = "use `into_result()` to handle potential execution errors"]
pub struct ExecutionFinalResult {
    /// Total gas burnt by the execution
    pub total_gas_burnt: Gas,

    pub(crate) status: FinalExecutionStatus,
    pub(crate) details: ExecutionDetails,
}

impl fmt::Debug for ExecutionFinalResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExecutionFinalResult")
            .field("total_gas_burnt", &self.total_gas_burnt)
            .field("transaction", &self.details.transaction)
            .field("receipts", &self.details.receipts)
            .field("status", &self.status)
            .finish()
    }
}

impl ExecutionFinalResult {
    pub(crate) fn from_view(view: FinalExecutionOutcomeView) -> Self {
        let total_gas_burnt = view.transaction_outcome.outcome.gas_burnt
            + view
                .receipts_outcome
                .iter()
                .map(|t| t.outcome.gas_burnt)
                .sum::<u64>();

        let transaction = view.transaction_outcome.into();
        let receipts = view
            .receipts_outcome
            .into_iter()
            .map(ExecutionOutcome::from)
            .collect();

        Self {
            total_gas_burnt,
            status: view.status,
            details: ExecutionDetails {
                transaction,
                receipts,
            },
        }
    }

    /// Converts this object into a [`Result`] holding either [`ExecutionSuccess`] or [`ExecutionFailure`].
    #[allow(clippy::result_large_err)]
    pub fn into_result(self) -> Result<ExecutionSuccess, ExecutionFailure> {
        match self.status {
            FinalExecutionStatus::SuccessValue(value) => Ok(ExecutionResult {
                total_gas_burnt: self.total_gas_burnt,
                value: Value::from_string(general_purpose::STANDARD.encode(value)),
                details: self.details,
            }),
            FinalExecutionStatus::Failure(tx_error) => Err(ExecutionResult {
                total_gas_burnt: self.total_gas_burnt,
                value: tx_error,
                details: self.details,
            }),
            _ => unreachable!(),
        }
    }

    /// Returns the contained Ok value, consuming the self value.
    ///
    /// Because this function may panic, its use is generally discouraged. Instead, prefer
    /// to call into [`into_result`] then pattern matching and handle the Err case explicitly.
    ///
    /// [`into_result`]: crate::result::ExecutionFinalResult::into_result
    pub fn unwrap(self) -> ExecutionSuccess {
        self.into_result().unwrap()
    }

    /// Deserialize an instance of type `T` from bytes of JSON text sourced from the
    /// execution result of this call. This conversion can fail if the structure of
    /// the internal state does not meet up with [`serde::de::DeserializeOwned`]'s
    /// requirements.
    pub fn json<T: serde::de::DeserializeOwned>(self) -> Result<T> {
        let val = self.into_result()?;
        match val.json() {
            Err(err) => {
                // This catches the case: `EOF while parsing a value at line 1 column 0`
                // for a function that doesn't return anything; this is a more descriptive error.
                if *err.kind() == ErrorKind::DataConversion && val.value.repr.is_empty() {
                    return Err(ErrorKind::DataConversion.custom(
                        "the function call returned an empty value, which cannot be parsed as JSON",
                    ));
                }

                Err(err)
            }
            ok => ok,
        }
    }

    /// Deserialize an instance of type `T` from bytes sourced from the execution
    /// result. This conversion can fail if the structure of the internal state does
    /// not meet up with [`borsh::BorshDeserialize`]'s requirements.
    pub fn borsh<T: borsh::BorshDeserialize>(self) -> Result<T> {
        self.into_result()?.borsh()
    }

    /// Grab the underlying raw bytes returned from calling into a contract's function.
    /// If we want to deserialize these bytes into a rust datatype, use [`ExecutionResult::json`]
    /// or [`ExecutionResult::borsh`] instead.
    pub fn raw_bytes(self) -> Result<Vec<u8>> {
        self.into_result()?.raw_bytes()
    }

    /// Checks whether the transaction was successful. Returns true if
    /// the transaction has a status of [`FinalExecutionStatus::SuccessValue`].
    pub fn is_success(&self) -> bool {
        matches!(self.status, FinalExecutionStatus::SuccessValue(_))
    }

    /// Checks whether the transaction has failed. Returns true if
    /// the transaction has a status of [`FinalExecutionStatus::Failure`].
    pub fn is_failure(&self) -> bool {
        matches!(self.status, FinalExecutionStatus::Failure(_))
    }

    /// Returns just the transaction outcome.
    pub fn outcome(&self) -> &ExecutionOutcome {
        self.details.outcome()
    }

    /// Grab all outcomes after the execution of the transaction. This includes outcomes
    /// from the transaction and all the receipts it generated.
    pub fn outcomes(&self) -> Vec<&ExecutionOutcome> {
        self.details.outcomes()
    }

    /// Grab all outcomes after the execution of the transaction. This includes outcomes
    /// only from receipts generated by this transaction.
    pub fn receipt_outcomes(&self) -> &[ExecutionOutcome] {
        self.details.receipt_outcomes()
    }

    /// Grab all outcomes that did not succeed the execution of this transaction. This
    /// will also include the failures from receipts as well.
    pub fn failures(&self) -> Vec<&ExecutionOutcome> {
        self.details.failures()
    }

    /// Just like `failures`, grab only failed receipt outcomes.
    pub fn receipt_failures(&self) -> Vec<&ExecutionOutcome> {
        self.details.receipt_failures()
    }

    /// Grab all logs from both the transaction and receipt outcomes.
    pub fn logs(&self) -> Vec<&str> {
        self.details.logs()
    }
}

impl ExecutionSuccess {
    /// Deserialize an instance of type `T` from bytes of JSON text sourced from the
    /// execution result of this call. This conversion can fail if the structure of
    /// the internal state does not meet up with [`serde::de::DeserializeOwned`]'s
    /// requirements.
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T> {
        self.value.json()
    }

    /// Deserialize an instance of type `T` from bytes sourced from the execution
    /// result. This conversion can fail if the structure of the internal state does
    /// not meet up with [`borsh::BorshDeserialize`]'s requirements.
    pub fn borsh<T: borsh::BorshDeserialize>(&self) -> Result<T> {
        self.value.borsh()
    }

    /// Grab the underlying raw bytes returned from calling into a contract's function.
    /// If we want to deserialize these bytes into a rust datatype, use [`ExecutionResult::json`]
    /// or [`ExecutionResult::borsh`] instead.
    pub fn raw_bytes(&self) -> Result<Vec<u8>> {
        self.value.raw_bytes()
    }
}

impl<T> ExecutionResult<T> {
    /// Returns just the transaction outcome.
    pub fn outcome(&self) -> &ExecutionOutcome {
        self.details.outcome()
    }

    /// Grab all outcomes after the execution of the transaction. This includes outcomes
    /// from the transaction and all the receipts it generated.
    pub fn outcomes(&self) -> Vec<&ExecutionOutcome> {
        self.details.outcomes()
    }

    /// Grab all outcomes after the execution of the transaction. This includes outcomes
    /// only from receipts generated by this transaction.
    pub fn receipt_outcomes(&self) -> &[ExecutionOutcome] {
        self.details.receipt_outcomes()
    }

    /// Grab all outcomes that did not succeed the execution of this transaction. This
    /// will also include the failures from receipts as well.
    pub fn failures(&self) -> Vec<&ExecutionOutcome> {
        self.details.failures()
    }

    /// Just like `failures`, grab only failed receipt outcomes.
    pub fn receipt_failures(&self) -> Vec<&ExecutionOutcome> {
        self.details.receipt_failures()
    }

    /// Grab all logs from both the transaction and receipt outcomes.
    pub fn logs(&self) -> Vec<&str> {
        self.details.logs()
    }
}

/// The result from a call into a View function. This contains the contents or
/// the results from the view function call itself. The consumer of this object
/// can choose how to deserialize its contents.
#[derive(PartialEq, Eq, Clone, Debug)]
#[non_exhaustive]
pub struct ViewResultDetails {
    /// Our result from our call into a view function.
    pub result: Vec<u8>,
    /// Logs generated from the view function.
    pub logs: Vec<String>,
}

impl ViewResultDetails {
    /// Deserialize an instance of type `T` from bytes of JSON text sourced from the
    /// execution result of this call. This conversion can fail if the structure of
    /// the internal state does not meet up with [`serde::de::DeserializeOwned`]'s
    /// requirements.
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T> {
        serde_json::from_slice(&self.result).map_err(|e| ErrorKind::DataConversion.custom(e))
    }

    /// Deserialize an instance of type `T` from bytes sourced from this view call's
    /// result. This conversion can fail if the structure of the internal state does
    /// not meet up with [`borsh::BorshDeserialize`]'s requirements.
    pub fn borsh<T: borsh::BorshDeserialize>(&self) -> Result<T> {
        borsh::BorshDeserialize::try_from_slice(&self.result)
            .map_err(|e| ErrorKind::DataConversion.custom(e))
    }
}

impl From<CallResult> for ViewResultDetails {
    fn from(result: CallResult) -> Self {
        ViewResultDetails {
            result: result.result,
            logs: result.logs,
        }
    }
}

/// The execution outcome of a transaction. This type contains all data relevant to
/// calling into a function, and getting the results back.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ExecutionOutcome {
    /// The hash of the transaction that generated this outcome.
    pub transaction_hash: CryptoHash,
    /// The hash of the block that generated this outcome.
    pub block_hash: CryptoHash,
    /// Logs from this transaction or receipt.
    pub logs: Vec<String>,
    /// Receipt IDs generated by this transaction or receipt.
    pub receipt_ids: Vec<CryptoHash>,
    /// The amount of the gas burnt by the given transaction or receipt.
    pub gas_burnt: Gas,
    /// The amount of tokens burnt corresponding to the burnt gas amount.
    /// This value doesn't always equal to the `gas_burnt` multiplied by the gas price, because
    /// the prepaid gas price might be lower than the actual gas price and it creates a deficit.
    pub tokens_burnt: Balance,
    /// The id of the account on which the execution happens. For transaction this is signer_id,
    /// for receipt this is receiver_id.
    pub executor_id: AccountId,
    /// Execution status. Contains the result in case of successful execution.
    pub(crate) status: ExecutionStatusView,
}

impl ExecutionOutcome {
    /// Checks whether this execution outcome was a success. Returns true if a success value or
    /// receipt id is present.
    pub fn is_success(&self) -> bool {
        matches!(
            self.status,
            ExecutionStatusView::SuccessValue(_) | ExecutionStatusView::SuccessReceiptId(_)
        )
    }

    /// Checks whether this execution outcome was a failure. Returns true if it failed with
    /// an error or the execution state was unknown or pending.
    pub fn is_failure(&self) -> bool {
        matches!(
            self.status,
            ExecutionStatusView::Failure(_) | ExecutionStatusView::Unknown
        )
    }

    /// Converts this [`ExecutionOutcome`] into a Result type to match against whether the
    /// particular outcome has failed or not.
    pub fn into_result(self) -> Result<ValueOrReceiptId> {
        match self.status {
            ExecutionStatusView::SuccessValue(value) => Ok(ValueOrReceiptId::Value(
                Value::from_string(general_purpose::STANDARD.encode(value)),
            )),
            ExecutionStatusView::SuccessReceiptId(hash) => {
                Ok(ValueOrReceiptId::ReceiptId(CryptoHash(hash.0)))
            }
            ExecutionStatusView::Failure(err) => Err(ErrorKind::Execution.custom(err)),
            ExecutionStatusView::Unknown => {
                Err(ErrorKind::Execution.message("Execution pending or unknown"))
            }
        }
    }
}

/// Value or ReceiptId from a successful execution.
#[derive(Debug)]
pub enum ValueOrReceiptId {
    /// The final action succeeded and returned some value or an empty vec encoded in base64.
    Value(Value),
    /// The final action of the receipt returned a promise or the signed transaction was converted
    /// to a receipt. Contains the receipt_id of the generated receipt.
    ReceiptId(CryptoHash),
}

/// Value type returned from an [`ExecutionOutcome`] or receipt result. This value
/// can be converted into the underlying Rust datatype, or directly grab the raw
/// bytes associated to the value.
#[derive(Debug)]
pub struct Value {
    repr: String,
}

impl Value {
    fn from_string(value: String) -> Self {
        Self { repr: value }
    }

    /// Deserialize an instance of type `T` from bytes of JSON text sourced from the
    /// execution result of this call. This conversion can fail if the structure of
    /// the internal state does not meet up with [`serde::de::DeserializeOwned`]'s
    /// requirements.
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T> {
        let buf = self.raw_bytes()?;
        serde_json::from_slice(&buf).map_err(|e| ErrorKind::DataConversion.custom(e))
    }

    /// Deserialize an instance of type `T` from bytes sourced from the execution
    /// result. This conversion can fail if the structure of the internal state does
    /// not meet up with [`borsh::BorshDeserialize`]'s requirements.
    pub fn borsh<T: borsh::BorshDeserialize>(&self) -> Result<T> {
        let buf = self.raw_bytes()?;
        borsh::BorshDeserialize::try_from_slice(&buf)
            .map_err(|e| ErrorKind::DataConversion.custom(e))
    }

    /// Grab the underlying raw bytes returned from calling into a contract's function.
    /// If we want to deserialize these bytes into a rust datatype, use [`json`]
    /// or [`borsh`] instead.
    ///
    /// [`json`]: Value::json
    /// [`borsh`]: Value::borsh
    pub fn raw_bytes(&self) -> Result<Vec<u8>> {
        general_purpose::STANDARD
            .decode(&self.repr)
            .map_err(|e| ErrorKind::DataConversion.custom(e))
    }
}

impl From<ExecutionOutcomeWithIdView> for ExecutionOutcome {
    fn from(view: ExecutionOutcomeWithIdView) -> Self {
        ExecutionOutcome {
            transaction_hash: CryptoHash(view.id.0),
            block_hash: CryptoHash(view.block_hash.0),
            logs: view.outcome.logs,
            receipt_ids: view
                .outcome
                .receipt_ids
                .into_iter()
                .map(|c| CryptoHash(c.0))
                .collect(),
            gas_burnt: view.outcome.gas_burnt,
            tokens_burnt: view.outcome.tokens_burnt,
            executor_id: view.outcome.executor_id,
            status: view.outcome.status,
        }
    }
}
