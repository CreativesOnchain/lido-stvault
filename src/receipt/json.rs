use crate::error::Result;
use crate::models::VerificationReceipt;

pub fn generate_json_receipt(receipt: &VerificationReceipt) -> Result<String> {
    serde_json::to_string_pretty(receipt).map_err(Into::into)
}
