use poem_ext::patch_value::PatchValue;
use poem_openapi::Object;

#[derive(Debug, Clone, Object)]
pub struct UpdateSubtaskRequest {
    /// The number of morphcoins a user has to pay to access this subtask.
    pub fee: PatchValue<u64>,
}